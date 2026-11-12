//! type 1 font programs (`FontFile`): the eexec-encrypted private part,
//! its charstrings, and the charstring interpreter that turns them into
//! cubic outlines (adobe type 1 font format, chapters 6 to 8).

use std::collections::HashMap;

use super::outline::{Outline, PathOp};

/// a parsed type 1 program.
pub struct Type1Font {
    pub charstrings: HashMap<String, Vec<u8>>,
    subrs: Vec<Vec<u8>>,
    /// the program's own encoding (`dup code /name put`)
    pub encoding: HashMap<u32, String>,
    pub units_per_em: f64,
}

const EEXEC_KEY: u16 = 55665;
const CHARSTRING_KEY: u16 = 4330;

fn decrypt(data: &[u8], key: u16, skip: usize) -> Vec<u8> {
    let mut r = key;
    let mut plain = Vec::with_capacity(data.len());
    for &cipher in data {
        plain.push(cipher ^ (r >> 8) as u8);
        r = (u16::from(cipher).wrapping_add(r))
            .wrapping_mul(52845)
            .wrapping_add(22719);
    }
    plain.into_iter().skip(skip).collect()
}

fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack.windows(needle.len()).position(|w| w == needle)
}

/// the encrypted part may be written as hex digits instead of bytes.
fn encrypted_bytes(after_eexec: &[u8]) -> Vec<u8> {
    let start = after_eexec
        .iter()
        .position(|b| !b.is_ascii_whitespace())
        .unwrap_or(0);
    let body = &after_eexec[start..];
    let looks_hex = body.iter().take(4).all(u8::is_ascii_hexdigit);
    if !looks_hex {
        return body.to_vec();
    }
    let digits: Vec<u8> = body.iter().copied().filter(u8::is_ascii_hexdigit).collect();
    digits
        .as_chunks::<2>()
        .0
        .iter()
        .map(|pair| u8::from_str_radix(std::str::from_utf8(pair).unwrap_or("00"), 16).unwrap_or(0))
        .collect()
}

impl Type1Font {
    pub fn parse(data: &[u8]) -> Result<Self, String> {
        // a pfb file has segment headers; a pdf FontFile normally doesn't
        let data = strip_pfb(data);
        let eexec = find(&data, b"eexec").ok_or("no eexec section")?;
        let clear = &data[..eexec];
        let private = decrypt(&encrypted_bytes(&data[eexec + 5..]), EEXEC_KEY, 4);
        let len_iv = number_after(&private, b"/lenIV").unwrap_or(4.0) as usize;
        let units_per_em = font_matrix_units(clear);
        let subrs = read_subrs(&private, len_iv);
        let charstrings = read_charstrings(&private, len_iv);
        if charstrings.is_empty() {
            return Err("no charstrings".into());
        }
        Ok(Self {
            charstrings,
            subrs,
            encoding: read_encoding(clear),
            units_per_em,
        })
    }

    /// the outline of a named glyph in font units, y up.
    pub fn outline(&self, name: &str) -> Option<Outline> {
        let program = self.charstrings.get(name)?;
        let mut machine = Machine::new(self);
        machine.run(program, 0);
        Some(machine.outline)
    }
}

fn strip_pfb(data: &[u8]) -> Vec<u8> {
    if data.first() != Some(&0x80) {
        return data.to_vec();
    }
    let mut out = Vec::new();
    let mut index = 0;
    while index + 6 <= data.len() && data[index] == 0x80 && data[index + 1] != 3 {
        let length = u32::from_le_bytes([
            data[index + 2],
            data[index + 3],
            data[index + 4],
            data[index + 5],
        ]) as usize;
        let start = index + 6;
        let end = (start + length).min(data.len());
        out.extend_from_slice(&data[start..end]);
        index = end;
    }
    out
}

fn number_after(data: &[u8], key: &[u8]) -> Option<f64> {
    let at = find(data, key)? + key.len();
    let text: String = data[at..]
        .iter()
        .skip_while(|b| b.is_ascii_whitespace())
        .take_while(|b| b.is_ascii_digit() || matches!(b, b'.' | b'-'))
        .map(|&b| b as char)
        .collect();
    text.parse().ok()
}

/// units per em from `/FontMatrix [0.001 0 0 0.001 0 0]`.
fn font_matrix_units(clear: &[u8]) -> f64 {
    let Some(at) = find(clear, b"/FontMatrix") else {
        return 1000.0;
    };
    let rest = &clear[at + 11..];
    let open = rest.iter().position(|&b| b == b'[' || b == b'{');
    let first: String = open
        .map(|o| {
            rest[o + 1..]
                .iter()
                .skip_while(|b| b.is_ascii_whitespace())
                .take_while(|b| !b.is_ascii_whitespace() && **b != b']')
                .map(|&b| b as char)
                .collect()
        })
        .unwrap_or_default();
    match first.parse::<f64>() {
        Ok(scale) if scale > 0.0 => (1.0 / scale).round(),
        _ => 1000.0,
    }
}

/// `dup <code> /<name> put` entries of the clear-text part.
fn read_encoding(clear: &[u8]) -> HashMap<u32, String> {
    let mut encoding = HashMap::new();
    let text = String::from_utf8_lossy(clear);
    let words: Vec<&str> = text.split_whitespace().collect();
    for window in words.windows(4) {
        if window[0] == "dup" && window[3] == "put" && window[2].starts_with('/') {
            if let Ok(code) = window[1].parse::<u32>() {
                encoding.insert(code, window[2][1..].to_string());
            }
        }
    }
    encoding
}

/// reads `<length> RD <binary>` (or `-|`) at `position`, returning the
/// decrypted charstring and the position after it.
fn binary_after(private: &[u8], position: usize, len_iv: usize) -> Option<(Vec<u8>, usize)> {
    let mut index = position;
    let skip_space = |index: &mut usize| {
        while *index < private.len() && private[*index].is_ascii_whitespace() {
            *index += 1;
        }
    };
    skip_space(&mut index);
    let start = index;
    while index < private.len() && private[index].is_ascii_digit() {
        index += 1;
    }
    let length: usize = std::str::from_utf8(&private[start..index])
        .ok()?
        .parse()
        .ok()?;
    skip_space(&mut index);
    // the read-binary token: RD, -| or anything else up to one space
    while index < private.len() && !private[index].is_ascii_whitespace() {
        index += 1;
    }
    index += 1; // exactly one space before the binary data
    let end = index.checked_add(length)?;
    if end > private.len() {
        return None;
    }
    Some((decrypt(&private[index..end], CHARSTRING_KEY, len_iv), end))
}

fn read_subrs(private: &[u8], len_iv: usize) -> Vec<Vec<u8>> {
    let Some(at) = find(private, b"/Subrs") else {
        return Vec::new();
    };
    let count = number_after(private, b"/Subrs").unwrap_or(0.0) as usize;
    let mut subrs = vec![Vec::new(); count.min(65536)];
    let mut position = at;
    let end = find(private, b"/CharStrings").unwrap_or(private.len());
    while let Some(dup) = find(&private[position..end], b"dup ") {
        let mut index = position + dup + 4;
        let start = index;
        while index < private.len() && private[index].is_ascii_digit() {
            index += 1;
        }
        let Some(number) = std::str::from_utf8(&private[start..index])
            .ok()
            .and_then(|t| t.parse::<usize>().ok())
        else {
            position = index;
            continue;
        };
        match binary_after(private, index, len_iv) {
            Some((charstring, after)) => {
                if number < subrs.len() {
                    subrs[number] = charstring;
                }
                position = after;
            }
            None => break,
        }
    }
    subrs
}

fn read_charstrings(private: &[u8], len_iv: usize) -> HashMap<String, Vec<u8>> {
    let mut charstrings = HashMap::new();
    let Some(at) = find(private, b"/CharStrings") else {
        return charstrings;
    };
    let mut index = at + 12;
    // skip "<count> dict dup begin"
    if let Some(begin) = find(&private[index..], b"begin") {
        index += begin + 5;
    }
    while index < private.len() {
        while index < private.len() && private[index].is_ascii_whitespace() {
            index += 1;
        }
        if index >= private.len() || private[index] != b'/' {
            break;
        }
        let start = index + 1;
        index = start;
        while index < private.len() && !private[index].is_ascii_whitespace() {
            index += 1;
        }
        let name = String::from_utf8_lossy(&private[start..index]).into_owned();
        let Some((charstring, after)) = binary_after(private, index, len_iv) else {
            break;
        };
        charstrings.insert(name, charstring);
        index = after;
        // skip the closing token (ND or |-) on the same line
        while index < private.len() && private[index].is_ascii_whitespace() {
            index += 1;
        }
        while index < private.len() && !private[index].is_ascii_whitespace() {
            index += 1;
        }
    }
    charstrings
}

/// the type 1 charstring machine.
struct Machine<'a> {
    font: &'a Type1Font,
    stack: Vec<f64>,
    /// values handed back by othersubrs, read with `pop`
    ps_stack: Vec<f64>,
    x: f64,
    y: f64,
    open: bool,
    flexing: bool,
    flex_points: Vec<(f64, f64)>,
    outline: Outline,
    /// an offset for the accent of `seac`
    offset: (f64, f64),
    done: bool,
}

impl<'a> Machine<'a> {
    fn new(font: &'a Type1Font) -> Self {
        Self {
            font,
            stack: Vec::new(),
            ps_stack: Vec::new(),
            x: 0.0,
            y: 0.0,
            open: false,
            flexing: false,
            flex_points: Vec::new(),
            outline: Outline::default(),
            offset: (0.0, 0.0),
            done: false,
        }
    }

    fn point(&self) -> (f64, f64) {
        (self.x + self.offset.0, self.y + self.offset.1)
    }

    fn move_to(&mut self, dx: f64, dy: f64) {
        self.x += dx;
        self.y += dy;
        if self.flexing {
            self.flex_points.push((self.x, self.y));
            return;
        }
        if self.open {
            self.outline.ops.push(PathOp::Close);
        }
        let (x, y) = self.point();
        self.outline.ops.push(PathOp::Move(x, y));
        self.open = true;
    }

    fn line_to(&mut self, dx: f64, dy: f64) {
        self.x += dx;
        self.y += dy;
        let (x, y) = self.point();
        self.outline.ops.push(PathOp::Line(x, y));
    }

    fn curve_to(&mut self, d: [f64; 6]) {
        let (x1, y1) = (self.x + d[0], self.y + d[1]);
        let (x2, y2) = (x1 + d[2], y1 + d[3]);
        let (x3, y3) = (x2 + d[4], y2 + d[5]);
        self.x = x3;
        self.y = y3;
        let (ox, oy) = self.offset;
        self.outline.ops.push(PathOp::Cubic(
            x1 + ox,
            y1 + oy,
            x2 + ox,
            y2 + oy,
            x3 + ox,
            y3 + oy,
        ));
    }

    fn close(&mut self) {
        if self.open {
            self.outline.ops.push(PathOp::Close);
            self.open = false;
        }
    }

    fn pop(&mut self) -> f64 {
        self.stack.pop().unwrap_or(0.0)
    }

    fn run(&mut self, program: &[u8], depth: usize) {
        if depth > 10 {
            return;
        }
        let mut index = 0;
        while index < program.len() && !self.done {
            let byte = program[index];
            index += 1;
            match byte {
                32..=246 => self.stack.push(f64::from(byte) - 139.0),
                247..=250 => {
                    let next = f64::from(*program.get(index).unwrap_or(&0));
                    index += 1;
                    self.stack
                        .push((f64::from(byte) - 247.0) * 256.0 + next + 108.0);
                }
                251..=254 => {
                    let next = f64::from(*program.get(index).unwrap_or(&0));
                    index += 1;
                    self.stack
                        .push(-(f64::from(byte) - 251.0) * 256.0 - next - 108.0);
                }
                255 => {
                    let bytes: [u8; 4] = program
                        .get(index..index + 4)
                        .and_then(|b| b.try_into().ok())
                        .unwrap_or([0; 4]);
                    index += 4;
                    self.stack.push(f64::from(i32::from_be_bytes(bytes)));
                }
                12 => {
                    let escape = *program.get(index).unwrap_or(&0);
                    index += 1;
                    self.escape(escape, depth);
                }
                _ => self.command(byte, depth),
            }
        }
    }

    fn command(&mut self, byte: u8, depth: usize) {
        let args = std::mem::take(&mut self.stack);
        let a = |i: usize| args.get(i).copied().unwrap_or(0.0);
        match byte {
            13 => {
                // hsbw: side bearing and width
                self.x = a(0);
                self.y = 0.0;
            }
            21 => self.move_to(a(0), a(1)),
            22 => self.move_to(a(0), 0.0),
            4 => self.move_to(0.0, a(0)),
            5 => self.line_to(a(0), a(1)),
            6 => self.line_to(a(0), 0.0),
            7 => self.line_to(0.0, a(0)),
            8 => self.curve_to([a(0), a(1), a(2), a(3), a(4), a(5)]),
            30 => self.curve_to([0.0, a(0), a(1), a(2), a(3), 0.0]),
            31 => self.curve_to([a(0), 0.0, a(1), a(2), 0.0, a(3)]),
            9 => self.close(),
            10 => {
                // callsubr: the subroutine number is the last argument
                let mut args = args;
                let number = args.pop().unwrap_or(-1.0);
                self.stack = args;
                if number >= 0.0 {
                    if let Some(subr) = self.font.subrs.get(number as usize).cloned() {
                        self.run(&subr, depth + 1);
                    }
                }
            }
            11 => {
                // return: keep what the subroutine left for the caller
                self.stack = args;
            }
            14 => {
                self.close();
                self.done = true;
            }
            1 | 3 => {} // hints
            _ => {}
        }
    }

    fn escape(&mut self, escape: u8, depth: usize) {
        match escape {
            7 => {
                // sbw
                let args = std::mem::take(&mut self.stack);
                self.x = args.first().copied().unwrap_or(0.0);
                self.y = args.get(1).copied().unwrap_or(0.0);
            }
            6 => self.seac(depth),
            12 => {
                let b = self.pop();
                let a = self.pop();
                self.stack.push(if b != 0.0 { a / b } else { 0.0 });
            }
            16 => self.call_other_subr(),
            17 => {
                let value = self.ps_stack.pop().unwrap_or(0.0);
                self.stack.push(value);
            }
            33 => {
                let y = self.pop();
                let x = self.pop();
                self.x = x;
                self.y = y;
                self.stack.clear();
            }
            _ => self.stack.clear(), // dotsection, vstem3, hstem3
        }
    }

    fn call_other_subr(&mut self) {
        let number = self.pop() as i64;
        let count = self.pop().max(0.0) as usize;
        let mut args = Vec::new();
        for _ in 0..count {
            args.push(self.pop());
        }
        // args now holds the arguments last first, which is the order pop gives them back
        match number {
            1 => {
                self.flexing = true;
                self.flex_points.clear();
            }
            0 => {
                // the end of a flex: the first point is a reference, then two curves
                self.flexing = false;
                let points = std::mem::take(&mut self.flex_points);
                if points.len() >= 7 {
                    let (ox, oy) = self.offset;
                    for curve in [&points[1..4], &points[4..7]] {
                        self.outline.ops.push(PathOp::Cubic(
                            curve[0].0 + ox,
                            curve[0].1 + oy,
                            curve[1].0 + ox,
                            curve[1].1 + oy,
                            curve[2].0 + ox,
                            curve[2].1 + oy,
                        ));
                    }
                    self.x = points[6].0;
                    self.y = points[6].1;
                }
                // "pop pop setcurrentpoint" reads the end point back
                self.ps_stack.push(self.y);
                self.ps_stack.push(self.x);
            }
            _ => {
                // hint replacement (3) and others: hand the arguments back
                self.ps_stack.extend(args);
            }
        }
    }

    /// an accented character made of two standard encoding glyphs.
    fn seac(&mut self, depth: usize) {
        let achar = self.pop() as usize;
        let bchar = self.pop() as usize;
        let ady = self.pop();
        let adx = self.pop();
        let asb = self.pop();
        let names = &crate::pdf::encoding::base_encoding(None);
        let base = names
            .get(&(bchar as u32))
            .and_then(|n| self.font.charstrings.get(n))
            .cloned();
        let accent = names
            .get(&(achar as u32))
            .and_then(|n| self.font.charstrings.get(n))
            .cloned();
        if let Some(base) = base {
            let mut inner = Machine::new(self.font);
            inner.run(&base, depth + 1);
            self.outline.ops.extend(inner.outline.ops);
        }
        if let Some(accent) = accent {
            let mut inner = Machine::new(self.font);
            inner.offset = (adx - asb, ady);
            inner.run(&accent, depth + 1);
            self.outline.ops.extend(inner.outline.ops);
        }
        self.done = true;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn encrypt(plain: &[u8], key: u16) -> Vec<u8> {
        let mut r = key;
        plain
            .iter()
            .map(|&p| {
                let cipher = p ^ (r >> 8) as u8;
                r = (u16::from(cipher).wrapping_add(r))
                    .wrapping_mul(52845)
                    .wrapping_add(22719);
                cipher
            })
            .collect()
    }

    /// a type 1 number in charstring encoding (small values only).
    fn n(value: i32) -> Vec<u8> {
        if (-107..=107).contains(&value) {
            vec![(value + 139) as u8]
        } else if (108..=1131).contains(&value) {
            let v = value - 108;
            vec![(v / 256 + 247) as u8, (v % 256) as u8]
        } else {
            let mut bytes = vec![255];
            bytes.extend(value.to_be_bytes());
            bytes
        }
    }

    fn sample_font() -> Vec<u8> {
        // a square glyph "a": hsbw 0 500, rmoveto 0 0, rlineto 400 0, 0 400, -400 0, closepath, endchar
        let mut charstring = vec![0u8; 4]; // lenIV bytes
        for part in [
            n(0),
            n(500),
            vec![13],
            n(0),
            n(0),
            vec![21],
            n(400),
            n(0),
            vec![5],
            n(0),
            n(400),
            vec![5],
            n(-400),
            n(0),
            vec![5],
            vec![9, 14],
        ] {
            charstring.extend(part);
        }
        let encrypted_charstring = encrypt(&charstring, CHARSTRING_KEY);
        let mut private = b"xxxx dup /Private 8 dict dup begin /lenIV 4 def /Subrs 0 array\n/CharStrings 1 dict dup begin\n/a ".to_vec();
        private.extend(format!("{} RD ", encrypted_charstring.len()).as_bytes());
        private.extend(encrypted_charstring);
        private.extend(b" ND\nend end\n");
        let mut font = b"%!FontType1-1.0: Test\n/FontMatrix [0.001 0 0 0.001 0 0] readonly def\n/Encoding 256 array\ndup 97 /a put\nreadonly def\ncurrentfile eexec\n".to_vec();
        font.extend(encrypt(&private, EEXEC_KEY));
        font
    }

    #[test]
    fn a_square_glyph_reads_back() {
        let font = Type1Font::parse(&sample_font()).unwrap();
        assert_eq!(font.units_per_em, 1000.0);
        assert_eq!(font.encoding[&97], "a");
        let outline = font.outline("a").unwrap();
        assert_eq!(
            outline.ops,
            [
                PathOp::Move(0.0, 0.0),
                PathOp::Line(400.0, 0.0),
                PathOp::Line(400.0, 400.0),
                PathOp::Line(0.0, 400.0),
                PathOp::Close
            ]
        );
    }

    #[test]
    fn decryption_round_trips() {
        let plain = b"some private dictionary";
        assert_eq!(decrypt(&encrypt(plain, EEXEC_KEY), EEXEC_KEY, 0), plain);
    }
}

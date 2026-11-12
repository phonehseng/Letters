//! finding which glyphs an embedded Type1 font program holds.
//!
//! a Type1 program is mostly encrypted ("eexec"). the glyphs are listed
//! in its /CharStrings dictionary, after the encryption, each as
//! "/name length RD <binary> ND". decrypting is a simple, published
//! calculation (Adobe Type 1 Font Format, chapter 7), so the names can be
//! read without understanding the glyphs themselves.

use std::collections::HashSet;

/// the glyph names in a Type1 program, or none if it can't be read.
/// `clear_length` is the length of the unencrypted part (/Length1).
pub(crate) fn glyph_names(program: &[u8], clear_length: usize) -> Option<HashSet<String>> {
    let start = if clear_length > 0 && clear_length < program.len() {
        clear_length
    } else {
        find(program, b"eexec")? + 5
    };
    let mut encrypted = &program[start..];
    // the encrypted part starts after the line break following "eexec"
    while let Some((&first, rest)) = encrypted.split_first() {
        if matches!(first, b'\r' | b'\n' | b' ' | b'\t') {
            encrypted = rest;
        } else {
            break;
        }
    }
    let plain = decrypt(encrypted, 55665);
    let after = find(&plain, b"/CharStrings")?;
    read_charstring_names(&plain[after + 12..])
}

/// the eexec decryption: each byte is mixed with a running key. the first
/// four plain bytes are random padding and are dropped.
fn decrypt(cipher: &[u8], key: u16) -> Vec<u8> {
    let mut running = key;
    let plain = cipher.iter().map(|&byte| {
        let clear = byte ^ (running >> 8) as u8;
        running = (u16::from(byte).wrapping_add(running))
            .wrapping_mul(52845)
            .wrapping_add(22719);
        clear
    });
    plain.skip(4).collect()
}

fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

/// reads "/name length RD <length bytes>" entries until "end".
fn read_charstring_names(mut rest: &[u8]) -> Option<HashSet<String>> {
    let mut names = HashSet::new();
    loop {
        let Some(slash) = rest.iter().position(|byte| *byte == b'/') else {
            return (!names.is_empty()).then_some(names);
        };
        // stop at the end of the dictionary
        if find(&rest[..slash], b"end").is_some() && !names.is_empty() {
            return Some(names);
        }
        rest = &rest[slash + 1..];
        let name_end = rest.iter().position(|byte| byte.is_ascii_whitespace())?;
        let name = String::from_utf8_lossy(&rest[..name_end]).into_owned();
        rest = &rest[name_end..];
        let (length, after_length) = read_integer(rest)?;
        rest = after_length;
        // the RD (or -|) token and exactly one space, then the binary data
        let token_start = rest.iter().position(|byte| !byte.is_ascii_whitespace())?;
        let token_end = token_start
            + rest[token_start..]
                .iter()
                .position(|byte| byte.is_ascii_whitespace())?;
        let data_start = token_end + 1;
        rest = rest.get(data_start + length..)?;
        names.insert(name);
    }
}

fn read_integer(bytes: &[u8]) -> Option<(usize, &[u8])> {
    let start = bytes.iter().position(|byte| !byte.is_ascii_whitespace())?;
    let length = bytes[start..]
        .iter()
        .take_while(|byte| byte.is_ascii_digit())
        .count();
    let value = std::str::from_utf8(&bytes[start..start + length])
        .ok()?
        .parse()
        .ok()?;
    Some((value, &bytes[start + length..]))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// the eexec encryption, the reverse of `decrypt`, to build test data.
    fn encrypt(plain: &[u8]) -> Vec<u8> {
        let mut running: u16 = 55665;
        [0u8, 0, 0, 0]
            .iter()
            .chain(plain)
            .map(|&clear| {
                let cipher = clear ^ (running >> 8) as u8;
                running = (u16::from(cipher).wrapping_add(running))
                    .wrapping_mul(52845)
                    .wrapping_add(22719);
                cipher
            })
            .collect()
    }

    #[test]
    fn reads_the_glyph_names_after_decrypting() {
        let private = b"dup /Private 8 dict dup begin /CharStrings 3 dict dup begin\n\
            /.notdef 3 RD \x01\x02\x03 ND\n/a 2 RD /b ND\n/eacute 4 -| \x00\x00\x00\x00 |-\nend end";
        let mut program = b"%!PS-AdobeFont-1.0: Test\ncurrentfile eexec\n".to_vec();
        let clear_length = program.len();
        program.extend(encrypt(private));
        let names = glyph_names(&program, clear_length).unwrap();
        let mut sorted: Vec<&str> = names.iter().map(String::as_str).collect();
        sorted.sort_unstable();
        // "/b" inside the binary data of "a" is not mistaken for a glyph
        assert_eq!(sorted, [".notdef", "a", "eacute"]);
    }
}

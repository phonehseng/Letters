//! a small streaming xml reader. it hands out one event at a time
//! (start tag, end tag, text) without building a tree, so a sheet with
//! hundreds of thousands of rows is read in one pass with little memory,
//! and it knows each tag's byte position so unknown elements can be kept
//! exactly as written.
//!
//! like letters-office's reader, a doctype is skipped and its entities
//! are never expanded, so a hostile file cannot make it fetch anything
//! or blow up in memory.

use std::borrow::Cow;

/// deeper nesting is refused; real spreadsheets stay far below it
const MAX_DEPTH: usize = 256;

/// one start tag. attributes are read on request.
#[derive(Clone, Debug)]
pub struct Tag<'a> {
    /// the name as written, prefix included ("x:c", "table:table-cell")
    pub name: &'a str,
    attributes: &'a str,
    pub self_closing: bool,
    /// byte offsets of the tag's `<` and of the end of its `>`
    pub start: usize,
    pub end: usize,
}

impl<'a> Tag<'a> {
    /// the name without its prefix
    pub fn local(&self) -> &'a str {
        local_name(self.name)
    }

    /// every attribute, values unescaped.
    pub fn attributes(&self) -> Vec<(&'a str, Cow<'a, str>)> {
        let mut out = Vec::new();
        let mut rest = self.attributes;
        loop {
            rest = rest.trim_start();
            let Some(eq) = rest.find('=') else { break };
            let key = rest[..eq].trim();
            let after = rest[eq + 1..].trim_start();
            let Some(quote) = after.chars().next().filter(|c| *c == '"' || *c == '\'') else {
                break;
            };
            let Some(close) = after[1..].find(quote) else {
                break;
            };
            let raw = &after[1..1 + close];
            let value = unescape(raw).unwrap_or(Cow::Borrowed(raw));
            out.push((key, value));
            rest = &after[close + 2..];
        }
        out
    }

    /// an attribute by its full name ("r", "r:id") or, failing that, by
    /// local name.
    pub fn attr(&self, name: &str) -> Option<Cow<'a, str>> {
        let all = self.attributes();
        if let Some(index) = all.iter().position(|(key, _)| *key == name) {
            return Some(all.into_iter().nth(index)?.1);
        }
        let local = local_name(name);
        all.into_iter()
            .find(|(key, _)| local_name(key) == local && key.contains(':') == name.contains(':'))
            .map(|(_, value)| value)
    }

    /// owned attribute pairs, for keeping.
    pub fn owned_attributes(&self) -> Vec<(String, String)> {
        self.attributes()
            .into_iter()
            .map(|(key, value)| (key.to_string(), value.into_owned()))
            .collect()
    }
}

pub fn local_name(name: &str) -> &str {
    name.rsplit_once(':')
        .map(|(_, local)| local)
        .unwrap_or(name)
}

#[derive(Clone, Debug)]
pub enum Event<'a> {
    Start(Tag<'a>),
    /// end of an element; also sent right after a self-closing start tag.
    /// carries the local name and the offset just after the element
    End {
        local: &'a str,
        end: usize,
    },
    Text(Cow<'a, str>),
}

pub struct Reader<'a> {
    text: &'a str,
    pos: usize,
    depth: usize,
    pending_end: Option<(&'a str, usize)>,
}

impl<'a> Reader<'a> {
    pub fn new(text: &'a str) -> Reader<'a> {
        Reader {
            text,
            pos: 0,
            depth: 0,
            pending_end: None,
        }
    }

    pub fn source(&self) -> &'a str {
        self.text
    }

    /// the next event, none at the end of the text.
    #[allow(clippy::should_implement_trait)]
    pub fn next(&mut self) -> Result<Option<Event<'a>>, String> {
        if let Some((local, end)) = self.pending_end.take() {
            self.depth -= 1;
            return Ok(Some(Event::End { local, end }));
        }
        loop {
            if self.pos >= self.text.len() {
                if self.depth > 0 {
                    return Err("the file ends before every element is closed".into());
                }
                return Ok(None);
            }
            let rest = &self.text[self.pos..];
            if !rest.starts_with('<') {
                let length = rest.find('<').unwrap_or(rest.len());
                let raw = &rest[..length];
                self.pos += length;
                if self.depth == 0 {
                    if raw.trim().is_empty() {
                        continue;
                    }
                    return Err("there is text outside the root element".into());
                }
                return Ok(Some(Event::Text(unescape(raw)?)));
            }
            if rest.starts_with("<?") {
                self.skip_past("?>")?;
            } else if rest.starts_with("<!--") {
                self.skip_past("-->")?;
            } else if let Some(body) = rest.strip_prefix("<![CDATA[") {
                let Some(close) = body.find("]]>") else {
                    return Err("a cdata section is not closed".into());
                };
                let text = &body[..close];
                self.pos += 9 + close + 3;
                return Ok(Some(Event::Text(Cow::Borrowed(text))));
            } else if rest.starts_with("<!") {
                self.skip_doctype()?;
            } else if rest.starts_with("</") {
                let Some(close) = rest.find('>') else {
                    return Err("an end tag is not closed".into());
                };
                let name = rest[2..close].trim();
                self.pos += close + 1;
                if self.depth == 0 {
                    return Err(format!("</{name}> has no start tag"));
                }
                self.depth -= 1;
                return Ok(Some(Event::End {
                    local: local_name(name),
                    end: self.pos,
                }));
            } else {
                return self.start_tag().map(Some);
            }
        }
    }

    fn start_tag(&mut self) -> Result<Event<'a>, String> {
        let start = self.pos;
        let bytes = self.text.as_bytes();
        let mut index = start + 1;
        let mut quote: Option<u8> = None;
        // find the closing '>' outside quoted attribute values
        while index < bytes.len() {
            let byte = bytes[index];
            match quote {
                Some(q) if byte == q => quote = None,
                Some(_) => {}
                None if byte == b'"' || byte == b'\'' => quote = Some(byte),
                None if byte == b'>' => break,
                None => {}
            }
            index += 1;
        }
        if index >= bytes.len() {
            return Err("a tag is not closed".into());
        }
        let inner = &self.text[start + 1..index];
        let self_closing = inner.ends_with('/');
        let inner = inner.strip_suffix('/').unwrap_or(inner);
        let name_end = inner
            .find(|c: char| c.is_ascii_whitespace())
            .unwrap_or(inner.len());
        let name = &inner[..name_end];
        if name.is_empty() {
            return Err("a tag has no name".into());
        }
        self.pos = index + 1;
        self.depth += 1;
        if self.depth > MAX_DEPTH {
            return Err("elements are nested too deeply".into());
        }
        if self_closing {
            self.pending_end = Some((local_name(name), self.pos));
        }
        Ok(Event::Start(Tag {
            name,
            attributes: &inner[name_end..],
            self_closing,
            start,
            end: self.pos,
        }))
    }

    fn skip_past(&mut self, terminator: &str) -> Result<(), String> {
        match self.text[self.pos..].find(terminator) {
            Some(offset) => {
                self.pos += offset + terminator.len();
                Ok(())
            }
            None => Err(format!("a `{terminator}` is missing")),
        }
    }

    fn skip_doctype(&mut self) -> Result<(), String> {
        let mut depth = 0i32;
        for (offset, byte) in self.text[self.pos..].bytes().enumerate() {
            match byte {
                b'[' => depth += 1,
                b']' => depth -= 1,
                b'>' if depth <= 0 => {
                    self.pos += offset + 1;
                    return Ok(());
                }
                _ => {}
            }
        }
        Err("a declaration is not closed".into())
    }

    /// after a start tag: skips to the end of that element and returns
    /// its whole original text.
    pub fn skip_element(&mut self, tag: &Tag<'a>) -> Result<&'a str, String> {
        let end = self.element_end(tag)?;
        Ok(&self.text[tag.start..end])
    }

    /// after a start tag: skips to its end and returns the end offset.
    pub fn element_end(&mut self, tag: &Tag<'a>) -> Result<usize, String> {
        let mut depth = 1;
        loop {
            match self.next()? {
                None => return Err(format!("<{}> is not closed", tag.name)),
                Some(Event::Start(_)) => depth += 1,
                Some(Event::End { end, .. }) => {
                    depth -= 1;
                    if depth == 0 {
                        return Ok(end);
                    }
                }
                Some(Event::Text(_)) => {}
            }
        }
    }

    /// after a start tag: all the text inside the element, children's
    /// text included (rich text runs, `<t>` pieces).
    pub fn element_text(&mut self, tag: &Tag<'a>) -> Result<String, String> {
        let mut text = String::new();
        let mut depth = 1;
        // phonetic runs (furigana) are not part of the cell's text
        let mut skip_depth: Option<usize> = None;
        let _ = tag;
        loop {
            match self.next()? {
                None => return Err("an element is not closed".into()),
                Some(Event::Start(inner)) => {
                    depth += 1;
                    if skip_depth.is_none() && inner.local() == "rPh" {
                        skip_depth = Some(depth);
                    }
                }
                Some(Event::End { .. }) => {
                    if skip_depth == Some(depth) {
                        skip_depth = None;
                    }
                    depth -= 1;
                    if depth == 0 {
                        return Ok(text);
                    }
                }
                Some(Event::Text(part)) => {
                    if skip_depth.is_none() {
                        text.push_str(&part);
                    }
                }
            }
        }
    }

    /// reads until the root element starts and returns it.
    pub fn root(&mut self) -> Result<Tag<'a>, String> {
        loop {
            match self.next()? {
                Some(Event::Start(tag)) => return Ok(tag),
                Some(_) => {}
                None => return Err("there is no root element".into()),
            }
        }
    }
}

/// replaces entity and character references with their characters.
pub fn unescape(raw: &str) -> Result<Cow<'_, str>, String> {
    if !raw.contains('&') {
        return Ok(Cow::Borrowed(raw));
    }
    let mut out = String::with_capacity(raw.len());
    let mut rest = raw;
    while let Some(amp) = rest.find('&') {
        out.push_str(&rest[..amp]);
        let after = &rest[amp + 1..];
        let Some(semicolon) = after.find(';') else {
            return Err("a `&` is not part of an entity".into());
        };
        let entity = &after[..semicolon];
        let c = match entity {
            "lt" => Some('<'),
            "gt" => Some('>'),
            "amp" => Some('&'),
            "apos" => Some('\''),
            "quot" => Some('"'),
            _ if entity.starts_with("#x") || entity.starts_with("#X") => {
                u32::from_str_radix(&entity[2..], 16)
                    .ok()
                    .and_then(char::from_u32)
            }
            _ if entity.starts_with('#') => {
                entity[1..].parse::<u32>().ok().and_then(char::from_u32)
            }
            _ => None,
        };
        match c {
            Some(c) => out.push(c),
            None => return Err(format!("&{entity}; is not a known entity")),
        }
        rest = &after[semicolon + 1..];
    }
    out.push_str(rest);
    Ok(Cow::Owned(out))
}

/// bytes to text: utf-8 with or without a byte order mark, or utf-16
/// with one.
pub fn decode_text(bytes: &[u8]) -> Result<String, String> {
    if let Some(rest) = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]) {
        return String::from_utf8(rest.to_vec()).map_err(|error| error.to_string());
    }
    if bytes.starts_with(&[0xFF, 0xFE]) || bytes.starts_with(&[0xFE, 0xFF]) {
        let little = bytes[0] == 0xFF;
        let (pairs, _) = bytes[2..].as_chunks::<2>();
        let units: Vec<u16> = pairs
            .iter()
            .map(|pair| {
                if little {
                    u16::from_le_bytes(*pair)
                } else {
                    u16::from_be_bytes(*pair)
                }
            })
            .collect();
        return String::from_utf16(&units).map_err(|error| error.to_string());
    }
    String::from_utf8(bytes.to_vec()).map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn events(text: &str) -> Vec<String> {
        let mut reader = Reader::new(text);
        let mut out = Vec::new();
        while let Some(event) = reader.next().unwrap() {
            out.push(match event {
                Event::Start(tag) => format!("<{}>", tag.local()),
                Event::End { local, .. } => format!("</{local}>"),
                Event::Text(text) => text.into_owned(),
            });
        }
        out
    }

    #[test]
    fn reads_tags_text_and_attributes() {
        let xml =
            r#"<?xml version="1.0"?><x:a b="1 &amp; 2" c='>'><x:b/>hi &lt;<![CDATA[<c>]]></x:a>"#;
        assert_eq!(
            events(xml),
            vec!["<a>", "<b>", "</b>", "hi <", "<c>", "</a>"]
        );
        let mut reader = Reader::new(xml);
        let root = reader.root().unwrap();
        assert_eq!(root.attr("b").unwrap(), "1 & 2");
        assert_eq!(root.attr("c").unwrap(), ">");
    }

    #[test]
    fn keeps_original_text_of_skipped_elements() {
        let xml = "<root><keep  a = 'x' ><in/>t</keep ><next/></root>";
        let mut reader = Reader::new(xml);
        reader.root().unwrap();
        let Some(Event::Start(tag)) = reader.next().unwrap() else {
            panic!()
        };
        assert_eq!(
            reader.skip_element(&tag).unwrap(),
            "<keep  a = 'x' ><in/>t</keep >"
        );
    }

    #[test]
    fn broken_xml_is_an_error_not_a_panic() {
        for broken in [
            "<a>",
            "<a><b></a",
            "<a b='1>",
            "<a>&nope;</a>",
            "text",
            "<",
            "<a><!-- x",
        ] {
            let mut reader = Reader::new(broken);
            let mut failed = false;
            for _ in 0..100 {
                match reader.next() {
                    Err(_) => {
                        failed = true;
                        break;
                    }
                    Ok(None) => break,
                    Ok(Some(_)) => {}
                }
            }
            assert!(failed, "{broken:?} should fail");
        }
        let deep = "<a>".repeat(1000);
        let mut reader = Reader::new(&deep);
        assert!((0..1000).any(|_| reader.next().is_err()));
    }
}

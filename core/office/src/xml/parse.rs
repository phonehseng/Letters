//! turns xml text into a tree of elements, remembering each element's
//! byte range in the source.
//!
//! it handles what office files contain: elements, attributes, text,
//! cdata, comments, processing instructions and the five predefined
//! entities plus numeric character references. a doctype is skipped and
//! its entities are never expanded, so a hostile file cannot make the
//! reader fetch anything or blow up in memory.

use super::canonical_prefix;

/// deeper nesting than this is refused. real documents stay far below
/// it (a table in a table in a table is about 20 levels), and the limit
/// keeps the recursive readers well inside the stack.
const MAX_DEPTH: usize = 400;

#[derive(Clone, Debug, PartialEq)]
pub enum Node {
    Element(Element),
    Text(String),
}

#[derive(Clone, Debug, PartialEq)]
pub struct Element {
    /// the qualified name with the normalised prefix, e.g. "w:p"
    pub name: String,
    /// attribute names normalised the same way; namespace declarations
    /// are kept as written ("xmlns:w")
    pub attributes: Vec<(String, String)>,
    pub children: Vec<Node>,
    /// where the element starts (its `<`) and ends (after its last `>`)
    /// in the source text
    pub start: usize,
    pub end: usize,
}

impl Element {
    pub fn attr(&self, name: &str) -> Option<&str> {
        self.attributes
            .iter()
            .find(|(key, _)| key == name)
            .map(|(_, value)| value.as_str())
    }

    /// child elements, skipping text.
    pub fn elements(&self) -> impl Iterator<Item = &Element> {
        self.children.iter().filter_map(|node| match node {
            Node::Element(element) => Some(element),
            Node::Text(_) => None,
        })
    }

    pub fn child(&self, name: &str) -> Option<&Element> {
        self.elements().find(|element| element.name == name)
    }

    pub fn children_named<'a>(&'a self, name: &'a str) -> impl Iterator<Item = &'a Element> {
        self.elements().filter(move |element| element.name == name)
    }

    /// the attribute `w:val` of a child, a pattern word uses everywhere.
    pub fn child_val(&self, name: &str) -> Option<&str> {
        self.child(name).and_then(|child| child.attr("w:val"))
    }

    /// the text directly inside this element (not in its children).
    pub fn text(&self) -> String {
        let mut text = String::new();
        for node in &self.children {
            if let Node::Text(part) = node {
                text.push_str(part);
            }
        }
        text
    }

    /// all text inside this element and its descendants, in order.
    pub fn deep_text(&self) -> String {
        let mut text = String::new();
        self.collect_text(&mut text);
        text
    }

    fn collect_text(&self, out: &mut String) {
        for node in &self.children {
            match node {
                Node::Text(part) => out.push_str(part),
                Node::Element(element) => element.collect_text(out),
            }
        }
    }

    /// calls `visit` on this element and every element inside it.
    pub fn walk(&self, visit: &mut dyn FnMut(&Element)) {
        visit(self);
        for child in self.elements() {
            child.walk(visit);
        }
    }
}

/// a parsed file: the source text and its root element.
#[derive(Clone, Debug)]
pub struct XmlDocument {
    pub source: String,
    pub root: Element,
    /// the root's attributes exactly as written, namespace declarations
    /// included, so a rewritten part can declare the same prefixes
    pub root_attributes: Vec<(String, String)>,
}

impl XmlDocument {
    /// the original text of an element, byte for byte.
    pub fn raw(&self, element: &Element) -> &str {
        &self.source[element.start..element.end]
    }
}

/// bytes to text: utf-8 (with or without a byte order mark) or utf-16
/// with a byte order mark, the encodings office files use.
pub fn decode_text(bytes: &[u8]) -> Result<String, String> {
    if let Some(rest) = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]) {
        return String::from_utf8(rest.to_vec()).map_err(|error| error.to_string());
    }
    if bytes.starts_with(&[0xFF, 0xFE]) || bytes.starts_with(&[0xFE, 0xFF]) {
        let little = bytes[0] == 0xFF;
        let units: Vec<u16> = (2..bytes.len().saturating_sub(1))
            .step_by(2)
            .map(|index| {
                let pair = [bytes[index], bytes[index + 1]];
                if little {
                    u16::from_le_bytes(pair)
                } else {
                    u16::from_be_bytes(pair)
                }
            })
            .collect();
        return String::from_utf16(&units).map_err(|error| error.to_string());
    }
    String::from_utf8(bytes.to_vec()).map_err(|error| error.to_string())
}

/// one element being read, with what is needed to finish it.
struct Open {
    element: Element,
    written_name: String,
    namespaces_before: usize,
}

struct Parser<'a> {
    text: &'a str,
    pos: usize,
    /// prefix to uri, innermost last
    namespaces: Vec<(String, String)>,
    stack: Vec<Open>,
    root: Option<Element>,
    root_attributes: Vec<(String, String)>,
}

/// parses xml text into a tree. errors say what is wrong and where.
pub fn parse(text: &str) -> Result<XmlDocument, String> {
    let mut parser = Parser {
        text,
        pos: 0,
        namespaces: vec![(
            "xml".to_string(),
            "http://www.w3.org/XML/1998/namespace".to_string(),
        )],
        stack: Vec::new(),
        root: None,
        root_attributes: Vec::new(),
    };
    parser.run()?;
    let root = parser.root.ok_or("there is no root element")?;
    Ok(XmlDocument {
        source: text.to_string(),
        root,
        root_attributes: parser.root_attributes,
    })
}

impl<'a> Parser<'a> {
    fn run(&mut self) -> Result<(), String> {
        while self.pos < self.text.len() {
            let rest = &self.text[self.pos..];
            match rest.find('<') {
                Some(0) => self.markup()?,
                Some(offset) => self.text_until(self.pos + offset)?,
                None => self.text_until(self.text.len())?,
            }
        }
        if let Some(open) = self.stack.last() {
            return Err(format!(
                "the file ends before <{}> is closed",
                open.written_name
            ));
        }
        Ok(())
    }

    fn error_at(&self, message: &str) -> String {
        let line = self.text[..self.pos].matches('\n').count() + 1;
        format!("{message} (line {line})")
    }

    fn text_until(&mut self, end: usize) -> Result<(), String> {
        let raw = &self.text[self.pos..end];
        if self.stack.is_empty() {
            self.pos = end;
            return if raw.trim().is_empty() {
                Ok(())
            } else {
                Err(self.error_at("there is text outside the root element"))
            };
        }
        let text = unescape(raw).map_err(|message| self.error_at(&message))?;
        self.pos = end;
        if let Some(open) = self.stack.last_mut() {
            push_text(&mut open.element, text);
        }
        Ok(())
    }

    fn markup(&mut self) -> Result<(), String> {
        let rest = &self.text[self.pos..];
        if rest.starts_with("<?") {
            self.skip_past("?>")
        } else if rest.starts_with("<!--") {
            self.skip_past("-->")
        } else if rest.starts_with("<![CDATA[") {
            self.cdata()
        } else if rest.starts_with("<!") {
            self.skip_doctype()
        } else if rest.starts_with("</") {
            self.end_tag()
        } else {
            self.start_tag()
        }
    }

    fn skip_past(&mut self, terminator: &str) -> Result<(), String> {
        match self.text[self.pos..].find(terminator) {
            Some(offset) => {
                self.pos += offset + terminator.len();
                Ok(())
            }
            None => Err(self.error_at(&format!("a `{terminator}` is missing"))),
        }
    }

    fn cdata(&mut self) -> Result<(), String> {
        let body_start = self.pos + "<![CDATA[".len();
        let Some(offset) = self.text[body_start..].find("]]>") else {
            return Err(self.error_at("a cdata section is not closed"));
        };
        let body = self.text[body_start..body_start + offset].to_string();
        self.pos = body_start + offset + 3;
        match self.stack.last_mut() {
            Some(open) => {
                push_text(&mut open.element, body);
                Ok(())
            }
            None => Err(self.error_at("there is text outside the root element")),
        }
    }

    /// skips `<!DOCTYPE ...>` including an internal subset in brackets.
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
        Err(self.error_at("a declaration is not closed"))
    }

    fn end_tag(&mut self) -> Result<(), String> {
        let start = self.pos;
        let Some(offset) = self.text[start..].find('>') else {
            return Err(self.error_at("an end tag is not closed"));
        };
        let name = self.text[start + 2..start + offset].trim();
        self.pos = start + offset + 1;
        let Some(open) = self.stack.pop() else {
            return Err(self.error_at(&format!("</{name}> has no matching start tag")));
        };
        if open.written_name != name {
            return Err(self.error_at(&format!("</{name}> closes <{}>", open.written_name)));
        }
        let mut element = open.element;
        element.end = self.pos;
        self.namespaces.truncate(open.namespaces_before);
        self.finish(element)
    }

    fn start_tag(&mut self) -> Result<(), String> {
        let start = self.pos;
        let (written_name, raw_attributes, self_closing, end) = self.read_tag(start)?;
        self.pos = end;
        if self.stack.len() >= MAX_DEPTH {
            return Err(self.error_at("elements are nested too deeply"));
        }
        if self.root.is_some() && self.stack.is_empty() {
            return Err(self.error_at("there is a second root element"));
        }
        let namespaces_before = self.namespaces.len();
        for (key, value) in &raw_attributes {
            if key == "xmlns" {
                self.namespaces.push((String::new(), value.clone()));
            } else if let Some(prefix) = key.strip_prefix("xmlns:") {
                self.namespaces.push((prefix.to_string(), value.clone()));
            }
        }
        let attributes = raw_attributes
            .iter()
            .map(|(key, value)| (self.attribute_name(key), value.clone()))
            .collect();
        if self.stack.is_empty() {
            self.root_attributes = raw_attributes;
        }
        let element = Element {
            name: self.element_name(&written_name),
            attributes,
            children: Vec::new(),
            start,
            end,
        };
        if self_closing {
            self.namespaces.truncate(namespaces_before);
            self.finish(element)
        } else {
            self.stack.push(Open {
                element,
                written_name,
                namespaces_before,
            });
            Ok(())
        }
    }

    /// reads `<name a="1" b='2'>` or `<name/>`. returns the name, the
    /// attributes as written (values unescaped), whether it closed itself
    /// and where the tag ends.
    #[allow(clippy::type_complexity)]
    fn read_tag(
        &self,
        start: usize,
    ) -> Result<(String, Vec<(String, String)>, bool, usize), String> {
        let bytes = self.text.as_bytes();
        let mut index = start + 1;
        let name_start = index;
        while index < bytes.len() && !is_name_end(bytes[index]) {
            index += 1;
        }
        let name = self.text[name_start..index].to_string();
        if name.is_empty() {
            return Err(self.error_at("a tag has no name"));
        }
        let mut attributes = Vec::new();
        loop {
            while index < bytes.len() && bytes[index].is_ascii_whitespace() {
                index += 1;
            }
            match bytes.get(index) {
                None => return Err(self.error_at(&format!("<{name}> is not closed"))),
                Some(b'>') => return Ok((name, attributes, false, index + 1)),
                Some(b'/') if bytes.get(index + 1) == Some(&b'>') => {
                    return Ok((name, attributes, true, index + 2))
                }
                Some(_) => {
                    let (key, value, next) = self.read_attribute(index, &name)?;
                    attributes.push((key, value));
                    index = next;
                }
            }
        }
    }

    fn read_attribute(&self, start: usize, tag: &str) -> Result<(String, String, usize), String> {
        let bytes = self.text.as_bytes();
        let mut index = start;
        while index < bytes.len() && !is_name_end(bytes[index]) && bytes[index] != b'=' {
            index += 1;
        }
        let key = self.text[start..index].to_string();
        while index < bytes.len() && bytes[index].is_ascii_whitespace() {
            index += 1;
        }
        if key.is_empty() || bytes.get(index) != Some(&b'=') {
            return Err(self.error_at(&format!("an attribute of <{tag}> is malformed")));
        }
        index += 1;
        while index < bytes.len() && bytes[index].is_ascii_whitespace() {
            index += 1;
        }
        let quote = match bytes.get(index) {
            Some(&quote) if quote == b'"' || quote == b'\'' => quote,
            _ => return Err(self.error_at(&format!("the value of {key} is not quoted"))),
        };
        let value_start = index + 1;
        let Some(length) = self.text[value_start..].find(quote as char) else {
            return Err(self.error_at(&format!("the value of {key} is not closed")));
        };
        let raw = &self.text[value_start..value_start + length];
        let value = unescape(raw).map_err(|message| self.error_at(&message))?;
        // attribute values treat tabs and line breaks as spaces
        let value = value.replace(['\t', '\n', '\r'], " ");
        Ok((key, value, value_start + length + 1))
    }

    fn resolve(&self, prefix: &str) -> Option<&str> {
        self.namespaces
            .iter()
            .rev()
            .find(|(known, _)| known == prefix)
            .map(|(_, uri)| uri.as_str())
    }

    fn element_name(&self, written: &str) -> String {
        let (prefix, local) = match written.split_once(':') {
            Some((prefix, local)) => (prefix, local),
            None => ("", written),
        };
        self.normalise(prefix, local, written)
    }

    fn attribute_name(&self, written: &str) -> String {
        if written == "xmlns" || written.starts_with("xmlns:") {
            return written.to_string();
        }
        match written.split_once(':') {
            // unprefixed attributes belong to no namespace
            None => written.to_string(),
            Some((prefix, local)) => self.normalise(prefix, local, written),
        }
    }

    fn normalise(&self, prefix: &str, local: &str, written: &str) -> String {
        match self.resolve(prefix).and_then(canonical_prefix) {
            Some(canonical) => format!("{canonical}:{local}"),
            None if prefix.is_empty() => local.to_string(),
            None => written.to_string(),
        }
    }

    fn finish(&mut self, element: Element) -> Result<(), String> {
        match self.stack.last_mut() {
            Some(parent) => parent.element.children.push(Node::Element(element)),
            None => self.root = Some(element),
        }
        Ok(())
    }
}

fn push_text(element: &mut Element, text: String) {
    if let Some(Node::Text(last)) = element.children.last_mut() {
        last.push_str(&text);
    } else {
        element.children.push(Node::Text(text));
    }
}

fn is_name_end(byte: u8) -> bool {
    byte.is_ascii_whitespace() || byte == b'>' || byte == b'/' || byte == b'='
}

/// replaces entity and character references with the characters they stand for.
fn unescape(raw: &str) -> Result<String, String> {
    if !raw.contains('&') {
        return Ok(raw.to_string());
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
        out.push(entity_char(entity)?);
        rest = &after[semicolon + 1..];
    }
    out.push_str(rest);
    Ok(out)
}

fn entity_char(entity: &str) -> Result<char, String> {
    let code = match entity {
        "lt" => return Ok('<'),
        "gt" => return Ok('>'),
        "amp" => return Ok('&'),
        "apos" => return Ok('\''),
        "quot" => return Ok('"'),
        _ if entity.starts_with("#x") || entity.starts_with("#X") => {
            u32::from_str_radix(&entity[2..], 16).ok()
        }
        _ if entity.starts_with('#') => entity[1..].parse::<u32>().ok(),
        _ => None,
    };
    code.and_then(char::from_u32)
        .ok_or_else(|| format!("&{entity}; is not a known entity"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_elements_attributes_and_text() {
        let xml = r#"<?xml version="1.0"?><root a="1"><child b='x &amp; y'>hi &lt;there&gt;</child><empty/></root>"#;
        let document = parse(xml).unwrap();
        assert_eq!(document.root.name, "root");
        assert_eq!(document.root.attr("a"), Some("1"));
        let child = document.root.child("child").unwrap();
        assert_eq!(child.attr("b"), Some("x & y"));
        assert_eq!(child.text(), "hi <there>");
        assert!(document.root.child("empty").is_some());
    }

    #[test]
    fn remembers_the_exact_source_of_each_element() {
        let xml = "<root><keep  odd = 'spacing' >text<!-- note --></keep ></root>";
        let document = parse(xml).unwrap();
        let keep = document.root.child("keep").unwrap();
        assert_eq!(
            document.raw(keep),
            "<keep  odd = 'spacing' >text<!-- note --></keep >"
        );
    }

    #[test]
    fn normalises_prefixes_by_namespace() {
        let xml = r#"<x:document xmlns:x="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><x:body x:val="1"/></x:document>"#;
        let document = parse(xml).unwrap();
        assert_eq!(document.root.name, "w:document");
        let body = document.root.child("w:body").unwrap();
        assert_eq!(body.attr("w:val"), Some("1"));
    }

    #[test]
    fn character_references_and_cdata() {
        let document = parse("<a>&#65;&#x42;<![CDATA[<c>]]></a>").unwrap();
        assert_eq!(document.root.text(), "AB<c>");
    }

    #[test]
    fn broken_xml_gives_errors_not_panics() {
        for broken in [
            "",
            "<a>",
            "<a></b>",
            "<a b=1/>",
            "<a b='1/>",
            "<a>&nope;</a>",
            "</a>",
            "<a/><b/>",
            "text",
            "<a><!-- open",
            "<",
            "<a b",
        ] {
            assert!(parse(broken).is_err(), "{broken:?} should fail");
        }
    }

    #[test]
    fn refuses_absurd_nesting() {
        let deep = "<a>".repeat(10_000) + &"</a>".repeat(10_000);
        assert!(parse(&deep).is_err());
    }

    #[test]
    fn doctype_entities_are_not_expanded() {
        let xml = r#"<!DOCTYPE a [<!ENTITY x "boom">]><a>&x;</a>"#;
        assert!(parse(xml).is_err());
    }

    #[test]
    fn decodes_utf16_and_bom() {
        let mut bytes = vec![0xFF, 0xFE];
        for unit in "<a>é</a>".encode_utf16() {
            bytes.extend_from_slice(&unit.to_le_bytes());
        }
        assert_eq!(decode_text(&bytes).unwrap(), "<a>é</a>");
        assert_eq!(decode_text(b"\xEF\xBB\xBF<a/>").unwrap(), "<a/>");
    }
}

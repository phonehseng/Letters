//! reading and writing xml text. the reader streams (see `reader`); the
//! writer pushes tags and text into a string, escaping every piece.

pub mod fragment;
pub mod reader;

pub use fragment::Fragment;

pub use reader::{decode_text, local_name, Event, Reader, Tag};

/// an xml text being built.
#[derive(Default)]
pub struct XmlWriter {
    out: String,
}

impl XmlWriter {
    pub fn new() -> XmlWriter {
        XmlWriter::default()
    }

    /// starts with the standard declaration.
    pub fn with_declaration() -> XmlWriter {
        XmlWriter {
            out: String::from("<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n"),
        }
    }

    pub fn open(&mut self, name: &str, attributes: &[(&str, &str)]) {
        self.tag(name, attributes.iter().copied(), false);
    }

    pub fn empty(&mut self, name: &str, attributes: &[(&str, &str)]) {
        self.tag(name, attributes.iter().copied(), true);
    }

    pub fn open_owned(&mut self, name: &str, attributes: &[(String, String)]) {
        self.tag(
            name,
            attributes.iter().map(|(k, v)| (k.as_str(), v.as_str())),
            false,
        );
    }

    pub fn empty_owned(&mut self, name: &str, attributes: &[(String, String)]) {
        self.tag(
            name,
            attributes.iter().map(|(k, v)| (k.as_str(), v.as_str())),
            true,
        );
    }

    fn tag<'a>(
        &mut self,
        name: &str,
        attributes: impl Iterator<Item = (&'a str, &'a str)>,
        empty: bool,
    ) {
        self.out.push('<');
        self.out.push_str(name);
        for (key, value) in attributes {
            self.out.push(' ');
            self.out.push_str(key);
            self.out.push_str("=\"");
            escape_attribute_into(value, &mut self.out);
            self.out.push('"');
        }
        self.out.push_str(if empty { "/>" } else { ">" });
    }

    pub fn close(&mut self, name: &str) {
        self.out.push_str("</");
        self.out.push_str(name);
        self.out.push('>');
    }

    pub fn text(&mut self, text: &str) {
        escape_text_into(text, &mut self.out);
    }

    /// `<name>text</name>`
    pub fn text_element(&mut self, name: &str, attributes: &[(&str, &str)], text: &str) {
        self.open(name, attributes);
        self.text(text);
        self.close(name);
    }

    /// xml that is already well formed (preserved xml)
    pub fn raw(&mut self, xml: &str) {
        self.out.push_str(xml);
    }

    pub fn len(&self) -> usize {
        self.out.len()
    }

    pub fn is_empty(&self) -> bool {
        self.out.is_empty()
    }

    pub fn finish(self) -> String {
        self.out
    }
}

/// escapes text between tags, dropping characters xml 1.0 cannot hold
/// (most control characters), which would make the whole file unreadable.
pub fn escape_text(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    escape_text_into(text, &mut out);
    out
}

fn escape_text_into(text: &str, out: &mut String) {
    for c in text.chars() {
        match c {
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '&' => out.push_str("&amp;"),
            c if is_xml_char(c) => out.push(c),
            _ => {}
        }
    }
}

pub fn escape_attribute(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    escape_attribute_into(text, &mut out);
    out
}

fn escape_attribute_into(text: &str, out: &mut String) {
    for c in text.chars() {
        match c {
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '&' => out.push_str("&amp;"),
            '"' => out.push_str("&quot;"),
            // kept as references so they survive attribute normalisation
            '\t' => out.push_str("&#9;"),
            '\n' => out.push_str("&#10;"),
            '\r' => out.push_str("&#13;"),
            c if is_xml_char(c) => out.push(c),
            _ => {}
        }
    }
}

fn is_xml_char(c: char) -> bool {
    matches!(c, '\t' | '\n' | '\r' | '\u{20}'..='\u{D7FF}' | '\u{E000}'..='\u{FFFD}' | '\u{10000}'..)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escapes_and_drops_invalid_characters() {
        assert_eq!(escape_text("a<b & c\u{1}>"), "a&lt;b &amp; c&gt;");
        assert_eq!(escape_attribute("say \"hi\"\n"), "say &quot;hi&quot;&#10;");
        let mut writer = XmlWriter::new();
        writer.open("a", &[("x", "1")]);
        writer.empty("b", &[]);
        writer.text("t");
        writer.close("a");
        assert_eq!(writer.finish(), "<a x=\"1\"><b/>t</a>");
    }
}

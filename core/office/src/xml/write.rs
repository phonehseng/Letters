//! builds xml text. deliberately simple: writers push tags and text into
//! a string, and every piece of text goes through the escaping below.

/// an xml text being built.
#[derive(Default)]
pub struct XmlWriter {
    out: String,
}

impl XmlWriter {
    pub fn new() -> XmlWriter {
        XmlWriter::default()
    }

    /// starts the file with the standard declaration.
    pub fn with_declaration() -> XmlWriter {
        let mut writer = XmlWriter::new();
        writer
            .out
            .push_str("<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n");
        writer
    }

    pub fn open(&mut self, name: &str, attributes: &[(&str, &str)]) {
        self.tag(name, attributes, false);
    }

    pub fn empty(&mut self, name: &str, attributes: &[(&str, &str)]) {
        self.tag(name, attributes, true);
    }

    /// an opening tag with attributes owned as strings.
    pub fn open_owned(&mut self, name: &str, attributes: &[(String, String)]) {
        let borrowed: Vec<(&str, &str)> = attributes
            .iter()
            .map(|(key, value)| (key.as_str(), value.as_str()))
            .collect();
        self.tag(name, &borrowed, false);
    }

    pub fn empty_owned(&mut self, name: &str, attributes: &[(String, String)]) {
        let borrowed: Vec<(&str, &str)> = attributes
            .iter()
            .map(|(key, value)| (key.as_str(), value.as_str()))
            .collect();
        self.tag(name, &borrowed, true);
    }

    fn tag(&mut self, name: &str, attributes: &[(&str, &str)], empty: bool) {
        self.out.push('<');
        self.out.push_str(name);
        for (key, value) in attributes {
            self.out.push(' ');
            self.out.push_str(key);
            self.out.push_str("=\"");
            self.out.push_str(&escape_attribute(value));
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
        self.out.push_str(&escape_text(text));
    }

    /// `<name>text</name>`
    pub fn text_element(&mut self, name: &str, attributes: &[(&str, &str)], text: &str) {
        self.open(name, attributes);
        self.text(text);
        self.close(name);
    }

    /// xml that is already well formed (preserved xml, mostly).
    pub fn raw(&mut self, xml: &str) {
        self.out.push_str(xml);
    }

    pub fn finish(self) -> String {
        self.out
    }

    pub fn as_str(&self) -> &str {
        &self.out
    }
}

/// escapes text for use between tags, dropping characters xml 1.0 cannot
/// hold at all (most control characters), which would otherwise make the
/// whole file unreadable.
pub fn escape_text(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '&' => out.push_str("&amp;"),
            c if is_xml_char(c) => out.push(c),
            _ => {}
        }
    }
    out
}

/// escapes text for use inside a double-quoted attribute value.
pub fn escape_attribute(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
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
    out
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
    }

    #[test]
    fn builds_tags() {
        let mut writer = XmlWriter::new();
        writer.open("a", &[("x", "1")]);
        writer.empty("b", &[]);
        writer.text("t");
        writer.close("a");
        assert_eq!(writer.finish(), "<a x=\"1\"><b/>t</a>");
    }
}

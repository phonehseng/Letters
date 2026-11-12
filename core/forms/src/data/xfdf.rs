//! xfdf (iso 19444-1): form answers as xml. a very small reader that
//! understands only <field name> and <value>, which is all form data needs.

use super::{Answer, NameTree};
use crate::error::{FormError, FormResult};
use crate::read::FieldInfo;

/// fields may nest; deeper than this is not a real form.
const MAX_FIELD_DEPTH: usize = 32;

fn xml_escape(text: &str) -> String {
    let mut out = String::new();
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            // a bare carriage return would be turned into a line feed by xml readers
            '\r' => out.push_str("&#13;"),
            other => out.push(other),
        }
    }
    out
}

const XFDF_START: &str = "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<xfdf xmlns=\"http://ns.adobe.com/xfdf/\" xml:space=\"preserve\">\n<fields>\n";
const XFDF_END: &str = "</fields>\n</xfdf>\n";

pub(super) fn write_xfdf(answers: &[(FieldInfo, Answer)]) -> String {
    let tree = NameTree::build(answers);
    let mut out = String::from(XFDF_START);
    for (name, kid) in &tree.kids {
        write_field(name, kid, 1, &mut out);
    }
    out.push_str(XFDF_END);
    out
}

/// one <field>, its values and its kids, indented by depth.
fn write_field(name: &str, tree: &NameTree, depth: usize, out: &mut String) {
    let indent = "  ".repeat(depth);
    out.push_str(&format!("{indent}<field name=\"{}\">\n", xml_escape(name)));
    if let Some((_, answer)) = &tree.answer {
        let values = match answer {
            Answer::One(value) => vec![value.clone()],
            Answer::Many(values) => values.clone(),
            Answer::Empty => vec![String::new()],
        };
        for value in values {
            let escaped = xml_escape(&value);
            out.push_str(&format!("{indent}  <value>{escaped}</value>\n"));
        }
    }
    for (kid_name, kid) in &tree.kids {
        write_field(kid_name, kid, depth + 1, out);
    }
    out.push_str(&format!("{indent}</field>\n"));
}

/// what the reader is in the middle of while walking the xml.
#[derive(Default)]
struct XfdfReader {
    answers: Vec<(String, Answer)>,
    /// the names of the open <field> elements, outermost first
    names: Vec<String>,
    /// the values collected for each open field
    values: Vec<Vec<String>>,
    /// the text of an open <value>
    value: Option<String>,
    saw_xfdf: bool,
}

fn bad(why: &str) -> FormError {
    FormError::BadFormData(why.to_owned())
}

/// reads the answers in an xfdf file. other elements (annotations, ids)
/// are skipped.
pub(super) fn read_xfdf(xml: &str) -> FormResult<Vec<(String, Answer)>> {
    let mut reader = XfdfReader::default();
    let mut rest = xml;
    while let Some(open) = rest.find('<') {
        reader.text(&unescape(&rest[..open]));
        rest = reader.markup(&rest[open..])?;
    }
    if !reader.saw_xfdf {
        return Err(bad("it isn’t an XFDF file"));
    }
    Ok(reader.answers)
}

impl XfdfReader {
    fn text(&mut self, text: &str) {
        if let Some(value) = self.value.as_mut() {
            value.push_str(text);
        }
    }

    /// handles the markup at the start of `rest` and returns what follows it.
    fn markup<'a>(&mut self, rest: &'a str) -> FormResult<&'a str> {
        if let Some(after) = rest.strip_prefix("<!--") {
            let end = after
                .find("-->")
                .ok_or_else(|| bad("a comment isn’t closed"))?;
            return Ok(&after[end + 3..]);
        }
        if let Some(after) = rest.strip_prefix("<![CDATA[") {
            let end = after
                .find("]]>")
                .ok_or_else(|| bad("a CDATA section isn’t closed"))?;
            self.text(&after[..end]);
            return Ok(&after[end + 3..]);
        }
        let close = rest.find('>').ok_or_else(|| bad("a tag isn’t closed"))?;
        let tag = &rest[1..close];
        let after = &rest[close + 1..];
        if tag.starts_with('?') || tag.starts_with('!') {
            return Ok(after);
        }
        match tag.strip_prefix('/') {
            Some(name) => self.close(local_name(name.trim())),
            None => self.open(tag)?,
        }
        Ok(after)
    }

    fn open(&mut self, tag: &str) -> FormResult<()> {
        let self_closing = tag.ends_with('/');
        let tag = tag.trim_end_matches('/');
        let element = local_name(tag.split_whitespace().next().unwrap_or(""));
        match element {
            "xfdf" => self.saw_xfdf = true,
            "field" => {
                let name = attribute(tag, "name").ok_or_else(|| bad("a field has no name"))?;
                if self.names.len() > MAX_FIELD_DEPTH {
                    return Err(bad("fields nest too deeply"));
                }
                if !self_closing {
                    self.names.push(name);
                    self.values.push(Vec::new());
                }
            }
            "value" if !self.names.is_empty() => {
                if self_closing {
                    self.push_value(String::new());
                } else {
                    self.value = Some(String::new());
                }
            }
            _ => {}
        }
        Ok(())
    }

    fn close(&mut self, element: &str) {
        match element {
            "value" => {
                if let Some(value) = self.value.take() {
                    self.push_value(value);
                }
            }
            "field" => self.close_field(),
            _ => {}
        }
    }

    fn push_value(&mut self, value: String) {
        if let Some(list) = self.values.last_mut() {
            list.push(value);
        }
    }

    /// a field's full name is its names joined with dots. a parent field
    /// holds no value of its own, so it gives no answer.
    fn close_field(&mut self) {
        let full_name = self.names.join(".");
        self.names.pop();
        let list = self.values.pop().unwrap_or_default();
        let answer = match list.len() {
            0 => return,
            1 => Answer::One(list.into_iter().next().unwrap_or_default()),
            _ => Answer::Many(list),
        };
        self.answers.push((full_name, answer));
    }
}

/// "xfdf:field" and "field" are the same element.
fn local_name(name: &str) -> &str {
    name.rsplit(':').next().unwrap_or(name)
}

/// the value of an attribute in a tag's text, entities decoded.
fn attribute(tag: &str, wanted: &str) -> Option<String> {
    let mut rest = tag;
    while let Some(at) = rest.find(wanted) {
        let starts_a_word = at == 0 || rest[..at].ends_with(char::is_whitespace);
        let after = rest[at + wanted.len()..].trim_start();
        if starts_a_word {
            if let Some(value) = quoted_value(after) {
                return Some(unescape(value));
            }
        }
        rest = &rest[at + wanted.len()..];
    }
    None
}

/// the text inside the quotes of `= "text"`.
fn quoted_value(after_name: &str) -> Option<&str> {
    let after = after_name.strip_prefix('=')?.trim_start();
    let quote = after.chars().next()?;
    if quote != '"' && quote != '\'' {
        return None;
    }
    let end = after[1..].find(quote)?;
    Some(&after[1..1 + end])
}

/// the five named xml entities and numeric ones (&#233; &#xE9;).
fn unescape(text: &str) -> String {
    let mut out = String::new();
    let mut rest = text;
    while let Some(at) = rest.find('&') {
        out.push_str(&rest[..at]);
        rest = &rest[at..];
        // an entity is short; a lone & is kept as it is
        let Some(end) = rest.find(';').filter(|end| *end <= 10) else {
            out.push('&');
            rest = &rest[1..];
            continue;
        };
        match entity(&rest[1..end]) {
            Some(c) => out.push(c),
            None => out.push_str(&rest[..=end]),
        }
        rest = &rest[end + 1..];
    }
    out.push_str(rest);
    out
}

fn entity(name: &str) -> Option<char> {
    match name {
        "amp" => Some('&'),
        "lt" => Some('<'),
        "gt" => Some('>'),
        "quot" => Some('"'),
        "apos" => Some('\''),
        _ => {
            let number = name.strip_prefix('#')?;
            let code = match number.strip_prefix('x') {
                Some(hex) => u32::from_str_radix(hex, 16).ok()?,
                None => number.parse().ok()?,
            };
            char::from_u32(code)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn xml_entities_are_read() {
        let text = unescape("Fish &amp; chips &#233;t&#xE9; &lt;3");
        assert_eq!(text, "Fish & chips \u{e9}t\u{e9} <3");
        let name = attribute("field name=\"a&amp;b\" x='1'", "name");
        assert_eq!(name, Some("a&b".into()));
    }

    #[test]
    fn nested_fields_join_their_names() {
        let xml =
            "<xfdf><fields><field name=\"a\"><field name=\"b\"><value>1</value></field></field>\
                   <field name=\"list\"><value>x</value><value>y</value></field></fields></xfdf>";
        let answers = read_xfdf(xml).unwrap();
        assert_eq!(answers[0], ("a.b".to_owned(), Answer::One("1".into())));
        assert_eq!(answers[1].1, Answer::Many(vec!["x".into(), "y".into()]));
        assert!(read_xfdf("<other/>").is_err());
    }
}

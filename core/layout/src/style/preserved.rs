//! reading the few properties layout needs from xml the office model keeps
//! as text (widow control, contextual spacing, character spacing, kerning,
//! the paragraph mark's size...). only the element's name and attributes are
//! read; nothing here is a full xml parser.

/// one element of preserved xml: its name and attributes.
#[derive(Clone, Debug, PartialEq)]
pub struct Tag {
    pub name: String,
    pub attributes: Vec<(String, String)>,
}

impl Tag {
    pub fn attr(&self, name: &str) -> Option<&str> {
        self.attributes
            .iter()
            .find(|(key, _)| key == name)
            .map(|(_, value)| value.as_str())
    }

    /// an on/off property: present means on unless `w:val` says off
    /// (ecma-376 17.17.4, st_onoff).
    pub fn on(&self) -> bool {
        !matches!(self.attr("w:val"), Some("0" | "false" | "off"))
    }
}

/// every element start tag in a piece of xml, outermost first.
pub fn tags(xml: &str) -> Vec<Tag> {
    let mut found = Vec::new();
    let mut rest = xml;
    while let Some(start) = rest.find('<') {
        rest = &rest[start + 1..];
        if rest.starts_with('/') || rest.starts_with('?') || rest.starts_with('!') {
            continue;
        }
        let end = rest.find('>').unwrap_or(rest.len());
        found.push(parse_tag(&rest[..end]));
        rest = &rest[end.min(rest.len())..];
    }
    found
}

fn parse_tag(inside: &str) -> Tag {
    let inside = inside.trim_end_matches('/').trim();
    let name_end = inside
        .find(|c: char| c.is_whitespace())
        .unwrap_or(inside.len());
    let name = inside[..name_end].to_string();
    let mut attributes = Vec::new();
    let mut rest = &inside[name_end..];
    while let Some(equals) = rest.find('=') {
        let key = rest[..equals].trim().to_string();
        let after = rest[equals + 1..].trim_start();
        let quote = after.chars().next().unwrap_or('"');
        if quote != '"' && quote != '\'' {
            break;
        }
        let body = &after[1..];
        let close = body.find(quote).unwrap_or(body.len());
        attributes.push((key, body[..close].to_string()));
        rest = &body[(close + 1).min(body.len())..];
    }
    Tag { name, attributes }
}

/// the first element called `name` anywhere in the preserved snippets.
pub fn find<'a>(snippets: impl IntoIterator<Item = &'a String>, name: &str) -> Option<Tag> {
    snippets
        .into_iter()
        .flat_map(|xml| tags(xml))
        .find(|tag| tag.name == name)
}

/// a numeric `w:val` of the first element called `name`.
pub fn number<'a>(snippets: impl IntoIterator<Item = &'a String>, name: &str) -> Option<i32> {
    find(snippets, name)?.attr("w:val")?.trim().parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_names_and_attributes() {
        let found =
            tags(r#"<w:rPr><w:sz w:val="28"/><w:lang w:val='en-US' w:eastAsia="ja"/></w:rPr>"#);
        assert_eq!(found.len(), 3);
        assert_eq!(found[1].name, "w:sz");
        assert_eq!(found[1].attr("w:val"), Some("28"));
        assert_eq!(found[2].attr("w:eastAsia"), Some("ja"));
    }

    #[test]
    fn on_off_values() {
        let off = vec![r#"<w:widowControl w:val="0"/>"#.to_string()];
        assert!(!find(&off, "w:widowControl").unwrap().on());
        let on = vec!["<w:contextualSpacing/>".to_string()];
        assert!(find(&on, "w:contextualSpacing").unwrap().on());
        assert_eq!(number(&off, "w:widowControl"), Some(0));
        assert!(find(&on, "w:kern").is_none());
    }
}

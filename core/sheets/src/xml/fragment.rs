//! small tools for kept xml: splitting an element into its attributes and
//! child elements, and putting it back together with some of them
//! changed. used where the model reads part of an element (a sheet view's
//! zoom) and the rest must survive untouched (its other attributes).

use super::{escape_attribute, Event, Reader};

/// an element split into its pieces, children kept as original text.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct Fragment {
    pub name: String,
    pub attributes: Vec<(String, String)>,
    /// (local name, original xml) of each child element
    pub children: Vec<(String, String)>,
}

impl Fragment {
    /// splits an element's xml; none when it is not well formed.
    pub fn parse(xml: &str) -> Option<Fragment> {
        let mut reader = Reader::new(xml);
        let root = reader.root().ok()?;
        let mut fragment = Fragment {
            name: root.name.to_string(),
            attributes: root.owned_attributes(),
            children: Vec::new(),
        };
        if root.self_closing {
            return Some(fragment);
        }
        loop {
            match reader.next().ok()? {
                Some(Event::Start(tag)) => {
                    let raw = reader.skip_element(&tag).ok()?;
                    fragment
                        .children
                        .push((tag.local().to_string(), raw.to_string()));
                }
                Some(Event::End { .. }) | None => return Some(fragment),
                Some(Event::Text(_)) => {}
            }
        }
    }

    pub fn attribute(&self, name: &str) -> Option<&str> {
        self.attributes
            .iter()
            .find(|(key, _)| key == name)
            .map(|(_, value)| value.as_str())
    }

    /// sets or, with none, removes an attribute, keeping the others' order.
    pub fn set_attribute(&mut self, name: &str, value: Option<String>) {
        let position = self.attributes.iter().position(|(key, _)| key == name);
        match (position, value) {
            (Some(index), Some(value)) => self.attributes[index].1 = value,
            (Some(index), None) => {
                self.attributes.remove(index);
            }
            (None, Some(value)) => self.attributes.push((name.to_string(), value)),
            (None, None) => {}
        }
    }

    /// the children other than those with the given local names.
    pub fn children_except(&self, names: &[&str]) -> Vec<&(String, String)> {
        self.children
            .iter()
            .filter(|(name, _)| !names.contains(&name.as_str()))
            .collect()
    }

    pub fn to_xml(&self) -> String {
        let mut out = format!("<{}", self.name);
        for (key, value) in &self.attributes {
            out.push_str(&format!(" {key}=\"{}\"", escape_attribute(value)));
        }
        if self.children.is_empty() {
            out.push_str("/>");
            return out;
        }
        out.push('>');
        for (_, xml) in &self.children {
            out.push_str(xml);
        }
        out.push_str(&format!("</{}>", self.name));
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_and_rebuilds_with_changes() {
        let xml = r#"<sheetPr codeName="Sheet1" filterMode="0"><tabColor rgb="FFFF0000"/><outlinePr summaryBelow="0"/></sheetPr>"#;
        let mut fragment = Fragment::parse(xml).unwrap();
        assert_eq!(fragment.attribute("codeName"), Some("Sheet1"));
        assert_eq!(fragment.children.len(), 2);
        fragment.set_attribute("filterMode", None);
        fragment.children.retain(|(name, _)| name != "tabColor");
        assert_eq!(
            fragment.to_xml(),
            r#"<sheetPr codeName="Sheet1"><outlinePr summaryBelow="0"/></sheetPr>"#
        );
        assert!(Fragment::parse("<broken").is_none());
    }
}

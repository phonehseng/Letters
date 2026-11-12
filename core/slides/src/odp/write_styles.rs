//! the automatic styles a written page needs, each written once however
//! many objects use it, and the named styles (gradients) that have to
//! live in styles.xml.

use std::collections::HashMap;

use crate::xml::escape_attribute;

/// names given to styles this writer makes; the prefix keeps them apart
/// from the names of styles kept from the file
const PREFIX: &str = "Lt";

#[derive(Default)]
pub struct StyleRegistry {
    /// (family, properties xml) to name, so equal styles are shared
    names: HashMap<(String, String), String>,
    pub automatic: String,
    /// named styles for office:styles (gradients)
    pub named: String,
    counter: usize,
}

impl StyleRegistry {
    /// the name of an automatic style with these property elements.
    /// `parent` names a style it builds on (a master's presentation style).
    pub fn style(&mut self, family: &str, parent: Option<&str>, properties: &str) -> String {
        let key = (
            format!("{family}|{}", parent.unwrap_or("")),
            properties.to_string(),
        );
        if let Some(name) = self.names.get(&key) {
            return name.clone();
        }
        self.counter += 1;
        let name = format!("{PREFIX}{}{}", family_letter(family), self.counter);
        let parent = parent
            .map(|p| format!(" style:parent-style-name=\"{}\"", escape_attribute(p)))
            .unwrap_or_default();
        self.automatic.push_str(&format!(
            "<style:style style:name=\"{name}\" style:family=\"{family}\"{parent}>{properties}</style:style>"
        ));
        self.names.insert(key, name.clone());
        name
    }

    /// a list style given the xml of its levels.
    pub fn list(&mut self, levels: &str) -> String {
        let key = ("list".to_string(), levels.to_string());
        if let Some(name) = self.names.get(&key) {
            return name.clone();
        }
        self.counter += 1;
        let name = format!("{PREFIX}L{}", self.counter);
        self.automatic.push_str(&format!(
            "<text:list-style style:name=\"{name}\">{levels}</text:list-style>"
        ));
        self.names.insert(key, name.clone());
        name
    }

    /// a two-colour gradient, named in office:styles.
    pub fn gradient(&mut self, start: &str, end: &str, angle_degrees: f64, radial: bool) -> String {
        let style = if radial { "radial" } else { "linear" };
        let properties = format!(
            "draw:style=\"{style}\" draw:start-color=\"#{start}\" draw:end-color=\"#{end}\" draw:angle=\"{:.0}deg\" draw:border=\"0%\"",
            angle_degrees
        );
        let key = ("gradient".to_string(), properties.clone());
        if let Some(name) = self.names.get(&key) {
            return name.clone();
        }
        self.counter += 1;
        let name = format!("{PREFIX}Gradient{}", self.counter);
        self.named.push_str(&format!(
            "<draw:gradient draw:name=\"{name}\" {properties}/>"
        ));
        self.names.insert(key, name.clone());
        name
    }
}

fn family_letter(family: &str) -> &'static str {
    match family {
        "graphic" => "gr",
        "presentation" => "pr",
        "paragraph" => "P",
        "text" => "T",
        "drawing-page" => "dp",
        "table-column" => "co",
        "table-row" => "ro",
        "table-cell" => "ce",
        _ => "s",
    }
}

//! list definitions: what bullets and numbers look like at each level,
//! and which lists continue or restart their counting.
//!
//! this follows word's two-step design, which odt maps onto cleanly:
//! a `ListDefinition` says how each level looks; a `ListInstance` is one
//! use of a definition. as in word, instances of the same definition
//! share their counters and keep counting on from each other, unless an
//! instance has a start override for a level: then that level starts
//! again there. `Numbering::restart` makes such an instance.

use serde::{Deserialize, Serialize};

use super::block::{Alignment, ParagraphProps, Preserved};
use super::style::RunProps;

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Numbering {
    #[serde(default)]
    pub definitions: Vec<ListDefinition>,
    #[serde(default)]
    pub instances: Vec<ListInstance>,
    /// numbering xml the model does not understand (picture bullets...)
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub preserved: Vec<String>,
}

impl Numbering {
    pub fn instance(&self, id: &str) -> Option<&ListInstance> {
        self.instances.iter().find(|instance| instance.id == id)
    }

    pub fn definition(&self, id: &str) -> Option<&ListDefinition> {
        self.definitions
            .iter()
            .find(|definition| definition.id == id)
    }

    /// the level settings a paragraph in `list_id` at `level` uses.
    pub fn level(&self, list_id: &str, level: u8) -> Option<&ListLevel> {
        let instance = self.instance(list_id)?;
        let definition = self.definition(&instance.definition_id)?;
        definition.levels.iter().find(|item| item.level == level)
    }

    /// adds a list that starts counting again from the definition's first
    /// number, and returns its id. paragraphs moved to it restart at 1.
    pub fn restart(&mut self, definition_id: &str) -> String {
        let id = self.add_instance(definition_id);
        let start = self
            .definition(definition_id)
            .and_then(|definition| definition.levels.first())
            .map(|level| level.start)
            .unwrap_or(1);
        if let Some(instance) = self.instances.iter_mut().find(|i| i.id == id) {
            instance.overrides.push(LevelOverride {
                level: 0,
                start: Some(start),
                preserved: None,
            });
        }
        id
    }

    /// adds a new list using `definition_id` and returns its id. it
    /// continues the counting of other lists with the same definition.
    pub fn add_instance(&mut self, definition_id: &str) -> String {
        let next = self
            .instances
            .iter()
            .filter_map(|instance| instance.id.parse::<u32>().ok())
            .max()
            .unwrap_or(0)
            + 1;
        let id = next.to_string();
        self.instances.push(ListInstance {
            id: id.clone(),
            definition_id: definition_id.to_string(),
            overrides: Vec::new(),
            preserved: Preserved::default(),
        });
        id
    }
}

/// how a list looks at each of its levels.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ListDefinition {
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    pub levels: Vec<ListLevel>,
    #[serde(default, skip_serializing_if = "Preserved::is_empty")]
    pub preserved: Preserved,
}

impl ListDefinition {
    /// a nine level definition: bullets, or numbers in the usual
    /// outline pattern (1. a. i. 1. a. i. ...).
    pub fn standard(id: &str, numbered: bool) -> ListDefinition {
        let bullets = ["\u{2022}", "\u{25E6}", "\u{25AA}"];
        let formats = [
            NumberFormat::Decimal,
            NumberFormat::LowerLetter,
            NumberFormat::LowerRoman,
        ];
        let levels = (0..9u8)
            .map(|level| {
                let slot = usize::from(level % 3);
                let (format, text) = if numbered {
                    (formats[slot].clone(), format!("%{}.", level + 1))
                } else {
                    (NumberFormat::Bullet, bullets[slot].to_string())
                };
                let left = 720 * (i32::from(level) + 1);
                let mut paragraph = ParagraphProps::default();
                paragraph.indent.left = Some(left);
                paragraph.indent.hanging = Some(360);
                ListLevel {
                    level,
                    format,
                    text,
                    start: 1,
                    alignment: Some(Alignment::Left),
                    paragraph,
                    run: RunProps::default(),
                    preserved: Preserved::default(),
                }
            })
            .collect();
        ListDefinition {
            id: id.into(),
            name: None,
            levels,
            preserved: Preserved::default(),
        }
    }
}

/// one level of a list definition.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ListLevel {
    /// 0 is the outermost level
    pub level: u8,
    pub format: NumberFormat,
    /// the label pattern: "%1." shows the level 1 number and a dot,
    /// "%1.%2" shows two levels, a bullet level holds the bullet itself
    pub text: String,
    #[serde(default = "one")]
    pub start: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub alignment: Option<Alignment>,
    /// indents of paragraphs at this level
    #[serde(default, skip_serializing_if = "ParagraphProps::is_empty")]
    pub paragraph: ParagraphProps,
    /// formatting of the label (a bullet's font, say)
    #[serde(default, skip_serializing_if = "RunProps::is_empty")]
    pub run: RunProps,
    #[serde(default, skip_serializing_if = "Preserved::is_empty")]
    pub preserved: Preserved,
}

fn one() -> u32 {
    1
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum NumberFormat {
    Bullet,
    Decimal,
    /// 01, 02...
    DecimalZero,
    LowerLetter,
    UpperLetter,
    LowerRoman,
    UpperRoman,
    /// a label with no number (just the text)
    None,
    /// a format the model does not know by name, kept as the file wrote it
    Other(String),
}

/// one use of a list definition with its own counters.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ListInstance {
    pub id: String,
    pub definition_id: String,
    /// levels whose counting starts somewhere other than the definition says
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub overrides: Vec<LevelOverride>,
    #[serde(default, skip_serializing_if = "Preserved::is_empty")]
    pub preserved: Preserved,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LevelOverride {
    pub level: u8,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub start: Option<u32>,
    /// a whole replacement level definition, kept as original xml
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub preserved: Option<String>,
}

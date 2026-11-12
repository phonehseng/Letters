//! walking the tree of form fields.
//!
//! a pdf form is a tree. the top level is listed in /AcroForm /Fields; a
//! field can have /Kids that are more fields (their names are joined with
//! dots, "address.street") or the widgets that show the field on a page.
//! a field with a single widget is often stored as one merged dictionary.
//! some settings (/FT /Ff /V /DA /Q /MaxLen /Opt) may be written on a
//! parent and apply to all its children, so reads look up the tree.

use std::collections::HashSet;

use lopdf::{Document, Object, ObjectId};

use crate::error::{FormError, FormResult};
use crate::objects::{
    acroform, get, get_dict, get_integer, get_name, get_text, references_in, resolve,
};

/// how deep a field tree may go before it is treated as broken.
const MAX_DEPTH: usize = 32;

/// one field that holds a value (a leaf of the tree), with its widgets.
#[derive(Clone, Debug, PartialEq)]
pub struct FieldNode {
    pub id: ObjectId,
    /// the full name, parts joined with dots
    pub name: String,
    /// widget annotations that show this field, in file order
    pub widgets: Vec<ObjectId>,
    /// this field and then its parents, nearest first. used for inherited values.
    pub lineage: Vec<ObjectId>,
}

/// the four kinds of field pdf knows (/FT).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FieldType {
    Text,
    Button,
    Choice,
    Signature,
}

/// field flag bits (/Ff), named the way the pdf reference names them.
pub mod flags {
    pub const READ_ONLY: u32 = 1;
    pub const REQUIRED: u32 = 1 << 1;
    pub const MULTILINE: u32 = 1 << 12;
    pub const PASSWORD: u32 = 1 << 13;
    pub const NO_TOGGLE_TO_OFF: u32 = 1 << 14;
    pub const RADIO: u32 = 1 << 15;
    pub const PUSH_BUTTON: u32 = 1 << 16;
    pub const COMBO: u32 = 1 << 17;
    pub const EDIT: u32 = 1 << 18;
    pub const MULTI_SELECT: u32 = 1 << 21;

    pub const COMB: u32 = 1 << 24;
}

/// every field that holds a value, in the order the form lists them.
pub fn collect_fields(doc: &Document) -> Vec<FieldNode> {
    let Some(form) = acroform(doc) else {
        return Vec::new();
    };
    let mut walker = Walker {
        doc,
        seen: HashSet::new(),
        fields: Vec::new(),
    };
    for id in references_in(doc, form, b"Fields") {
        walker.visit(id, "", &[], 0);
    }
    walker.fields
}

struct Walker<'a> {
    doc: &'a Document,
    seen: HashSet<ObjectId>,
    fields: Vec<FieldNode>,
}

impl Walker<'_> {
    fn visit(&mut self, id: ObjectId, parent_name: &str, parents: &[ObjectId], depth: usize) {
        if depth > MAX_DEPTH || !self.seen.insert(id) {
            return;
        }
        let Ok(dict) = self.doc.get_dictionary(id) else {
            return;
        };
        let name = match get_text(self.doc, dict, b"T") {
            Some(part) if parent_name.is_empty() => part,
            Some(part) => format!("{parent_name}.{part}"),
            None => parent_name.to_owned(),
        };
        let mut lineage = vec![id];
        lineage.extend_from_slice(parents);

        let kids = references_in(self.doc, dict, b"Kids");
        let (child_fields, widgets): (Vec<ObjectId>, Vec<ObjectId>) =
            kids.into_iter().partition(|kid| self.is_field(*kid));

        for child in child_fields {
            self.visit(child, &name, &lineage, depth + 1);
        }
        let widgets = if dict.has(b"Kids") {
            widgets
        } else {
            // no kids: the field and its only widget are one dictionary
            vec![id]
        };
        if !widgets.is_empty() && !name.is_empty() {
            for widget in &widgets {
                self.seen.insert(*widget);
            }
            self.fields.push(FieldNode {
                id,
                name,
                widgets,
                lineage,
            });
        }
    }

    /// a kid is a field (rather than a widget) when it has its own name
    /// part or kids of its own.
    fn is_field(&self, id: ObjectId) -> bool {
        self.doc
            .get_dictionary(id)
            .map(|dict| dict.has(b"T") || dict.has(b"Kids"))
            .unwrap_or(false)
    }
}

/// finds a field by its full name.
pub fn find_field(doc: &Document, name: &str) -> FormResult<FieldNode> {
    if acroform(doc).is_none() {
        return Err(FormError::NoForm);
    }
    collect_fields(doc)
        .into_iter()
        .find(|field| field.name == name)
        .ok_or_else(|| FormError::UnknownField(name.to_owned()))
}

impl FieldNode {
    /// a value written on this field or the nearest parent that has it.
    pub fn inherited<'a>(&self, doc: &'a Document, key: &[u8]) -> Option<&'a Object> {
        self.lineage.iter().find_map(|id| {
            let dict = doc.get_dictionary(*id).ok()?;
            get(doc, dict, key)
        })
    }

    pub fn field_type(&self, doc: &Document) -> Option<FieldType> {
        match self.inherited(doc, b"FT")?.as_name().ok()? {
            b"Tx" => Some(FieldType::Text),
            b"Btn" => Some(FieldType::Button),
            b"Ch" => Some(FieldType::Choice),
            b"Sig" => Some(FieldType::Signature),
            _ => None,
        }
    }

    pub fn flags(&self, doc: &Document) -> u32 {
        match self.inherited(doc, b"Ff") {
            Some(Object::Integer(value)) => *value as u32,
            Some(Object::Real(value)) => *value as u32,
            _ => 0,
        }
    }

    pub fn has_flag(&self, doc: &Document, flag: u32) -> bool {
        self.flags(doc) & flag != 0
    }

    /// the default appearance string: the field's own, or the form's.
    pub fn default_appearance(&self, doc: &Document) -> Option<Vec<u8>> {
        let own = self
            .inherited(doc, b"DA")
            .and_then(|value| value.as_str().ok());
        let form = acroform(doc).and_then(|form| get(doc, form, b"DA")?.as_str().ok());
        own.or(form).map(<[u8]>::to_vec)
    }

    /// text alignment (/Q): 0 left, 1 centred, 2 right.
    pub fn quadding(&self, doc: &Document) -> i64 {
        let own = self.lineage.iter().find_map(|id| {
            let dict = doc.get_dictionary(*id).ok()?;
            get_integer(doc, dict, b"Q")
        });
        let form = acroform(doc).and_then(|form| get_integer(doc, form, b"Q"));
        own.or(form).unwrap_or(0).clamp(0, 2)
    }

    pub fn max_length(&self, doc: &Document) -> Option<u32> {
        let length = self.lineage.iter().find_map(|id| {
            let dict = doc.get_dictionary(*id).ok()?;
            get_integer(doc, dict, b"MaxLen")
        })?;
        (length > 0).then_some(length as u32)
    }
}

/// the names of the "on" appearance states of a checkbox or radio widget.
/// pdf lets the author pick these names; "Off" is always the off state.
pub fn on_states(doc: &Document, widget: ObjectId) -> Vec<Vec<u8>> {
    let Ok(dict) = doc.get_dictionary(widget) else {
        return Vec::new();
    };
    let mut states: Vec<Vec<u8>> = Vec::new();
    if let Some(appearances) = get_dict(doc, dict, b"AP") {
        for key in [b"N".as_slice(), b"D".as_slice()] {
            if let Some(state_dict) =
                get(doc, appearances, key).and_then(|value| value.as_dict().ok())
            {
                for (state, _) in state_dict.iter() {
                    if state.as_slice() != b"Off" && !states.contains(state) {
                        states.push(state.clone());
                    }
                }
            }
        }
    }
    if states.is_empty() {
        if let Some(current) = get_name(doc, dict, b"AS").filter(|state| state.as_slice() != b"Off")
        {
            states.push(current);
        }
    }
    states
}

/// the first on-state of a widget, "Yes" when it has none yet.
pub fn on_state(doc: &Document, widget: ObjectId) -> Vec<u8> {
    on_states(doc, widget)
        .into_iter()
        .next()
        .unwrap_or_else(|| b"Yes".to_vec())
}

/// reads the strings of an /Opt entry. each entry is either a plain
/// string or a pair [export value, shown text].
pub fn options_of(doc: &Document, field: &FieldNode) -> Vec<(String, String)> {
    let Some(Object::Array(entries)) = field.inherited(doc, b"Opt") else {
        return Vec::new();
    };
    entries
        .iter()
        .filter_map(|entry| match resolve(doc, entry) {
            Object::String(bytes, _) => {
                let text = crate::objects::decode_text_string(bytes);
                Some((text.clone(), text))
            }
            Object::Array(pair) if !pair.is_empty() => {
                let text_of = |object: &Object| match resolve(doc, object) {
                    Object::String(bytes, _) => Some(crate::objects::decode_text_string(bytes)),
                    _ => None,
                };
                let export = text_of(&pair[0])?;
                let shown = pair
                    .get(1)
                    .and_then(text_of)
                    .unwrap_or_else(|| export.clone());
                Some((export, shown))
            }
            _ => None,
        })
        .collect()
}

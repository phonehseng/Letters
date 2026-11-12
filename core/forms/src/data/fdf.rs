//! fdf (iso 32000 12.7.8): form answers in pdf object syntax. fdf files
//! have no cross-reference table, so one is rebuilt before lopdf reads them.

use std::collections::BTreeMap;

use lopdf::{Dictionary, Document, Object};

use super::{Answer, NameTree};
use crate::error::{FormError, FormResult};
use crate::objects::{decode_text_string, text_string};
use crate::read::{FieldInfo, FieldKind};

pub(super) fn write_fdf(answers: &[(FieldInfo, Answer)]) -> Vec<u8> {
    fn field_object(name: &str, tree: &NameTree) -> Object {
        let mut dict = Dictionary::new();
        dict.set("T", text_string(name));
        if let Some((kind, answer)) = &tree.answer {
            let value = match (kind, answer) {
                (FieldKind::Checkbox | FieldKind::RadioGroup, Answer::One(state)) => {
                    Object::Name(state.as_bytes().to_vec())
                }
                (_, Answer::One(value)) => text_string(value),
                (_, Answer::Many(values)) => {
                    Object::Array(values.iter().map(|value| text_string(value)).collect())
                }
                (_, Answer::Empty) => text_string(""),
            };
            dict.set("V", value);
        }
        if !tree.kids.is_empty() {
            dict.set(
                "Kids",
                Object::Array(
                    tree.kids
                        .iter()
                        .map(|(n, kid)| field_object(n, kid))
                        .collect(),
                ),
            );
        }
        Object::Dictionary(dict)
    }
    let tree = NameTree::build(answers);
    let fields: Vec<Object> = tree
        .kids
        .iter()
        .map(|(n, kid)| field_object(n, kid))
        .collect();
    let mut fdf = Dictionary::new();
    fdf.set("Fields", Object::Array(fields));
    let mut root = Dictionary::new();
    root.set("FDF", Object::Dictionary(fdf));
    let mut out = b"%FDF-1.2\n%\xE2\xE3\xCF\xD3\n1 0 obj\n".to_vec();
    let _ = lopdf::Writer::write_object(&mut out, &Object::Dictionary(root));
    out.extend_from_slice(b"\nendobj\ntrailer\n<< /Root 1 0 R >>\n%%EOF\n");
    out
}

/// fdf has no cross-reference table that lopdf could read, so one is made:
/// the header is swapped for a pdf one and each object's place noted.
pub(super) fn read_fdf(data: &[u8]) -> FormResult<Vec<(String, Answer)>> {
    let bad = |why: &str| FormError::BadFormData(why.to_owned());
    if !data.starts_with(b"%FDF-") {
        return Err(bad("it doesn’t start like an FDF file"));
    }
    let mut bytes = data.to_vec();
    bytes[1..4].copy_from_slice(b"PDF");
    let mut offsets: BTreeMap<u32, usize> = BTreeMap::new();
    let mut line_start = 0;
    for index in 0..=bytes.len() {
        if index == bytes.len() || bytes[index] == b'\n' || bytes[index] == b'\r' {
            let line = String::from_utf8_lossy(&bytes[line_start..index]).into_owned();
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 3 && parts[2].starts_with("obj") {
                if let (Ok(number), Ok(_)) = (parts[0].parse::<u32>(), parts[1].parse::<u16>()) {
                    offsets.insert(number, line_start);
                }
            }
            line_start = index + 1;
        }
    }
    let text = String::from_utf8_lossy(&bytes).into_owned();
    let trailer_at = text
        .rfind("trailer")
        .ok_or_else(|| bad("it has no trailer"))?;
    let after = &text[trailer_at..];
    let root = after
        .find("/Root")
        .map(|at| {
            after[at + 5..]
                .split_whitespace()
                .next()
                .unwrap_or("")
                .to_owned()
        })
        .and_then(|number| number.parse::<u32>().ok())
        .ok_or_else(|| bad("it has no root"))?;
    let size = offsets.keys().max().copied().unwrap_or(0) + 1;
    let mut rebuilt = bytes[..trailer_at].to_vec();
    let xref_at = rebuilt.len();
    rebuilt.extend_from_slice(format!("xref\n0 {size}\n0000000000 65535 f \n").as_bytes());
    for number in 1..size {
        match offsets.get(&number) {
            Some(offset) => {
                rebuilt.extend_from_slice(format!("{offset:010} 00000 n \n").as_bytes())
            }
            None => rebuilt.extend_from_slice(b"0000000000 65535 f \n"),
        }
    }
    rebuilt.extend_from_slice(
        format!("trailer\n<< /Size {size} /Root {root} 0 R >>\nstartxref\n{xref_at}\n%%EOF\n")
            .as_bytes(),
    );
    let doc = Document::load_mem(&rebuilt).map_err(|error| bad(&error.to_string()))?;
    let catalog = doc
        .get_dictionary((root, 0))
        .map_err(|_| bad("its root is missing"))?;
    let fdf =
        crate::objects::get_dict(&doc, catalog, b"FDF").ok_or_else(|| bad("it has no /FDF"))?;
    let mut out = Vec::new();
    for field in crate::objects::get_array(&doc, fdf, b"Fields")
        .into_iter()
        .flatten()
    {
        read_fdf_field(&doc, field, "", &mut out, 0);
    }
    Ok(out)
}

fn read_fdf_field(
    doc: &Document,
    field: &Object,
    parent: &str,
    out: &mut Vec<(String, Answer)>,
    depth: usize,
) {
    if depth > 32 {
        return;
    }
    let Ok(dict) = crate::objects::resolve(doc, field).as_dict() else {
        return;
    };
    let part = crate::objects::get_text(doc, dict, b"T").unwrap_or_default();
    let name = if parent.is_empty() {
        part
    } else {
        format!("{parent}.{part}")
    };
    if let Some(value) = crate::objects::get(doc, dict, b"V") {
        let text_of = |object: &Object| match crate::objects::resolve(doc, object) {
            Object::String(bytes, _) => Some(decode_text_string(bytes)),
            Object::Name(bytes) => Some(String::from_utf8_lossy(bytes).into_owned()),
            _ => None,
        };
        let answer = match value {
            Object::Array(items) => Answer::Many(items.iter().filter_map(text_of).collect()),
            other => text_of(other).map(Answer::One).unwrap_or(Answer::Empty),
        };
        out.push((name.clone(), answer));
    }
    for kid in crate::objects::get_array(doc, dict, b"Kids")
        .into_iter()
        .flatten()
    {
        read_fdf_field(doc, kid, &name, out, depth + 1);
    }
}

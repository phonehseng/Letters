//! drawingml pieces shared by every object: colours, fills, lines,
//! transforms, geometry and style references, read and written.

use std::collections::HashMap;

use crate::xml::{escape_attribute, Element};

use super::rels::Rels;
mod color;
mod fill;
mod geometry;

pub use color::{find_color, read_color, write_color};
pub use fill::{find_fill, read_fill, read_line, write_fill, write_line, FILL_NAMES};
pub use geometry::{read_geometry, read_style, read_xfrm, write_geometry, write_style, write_xfrm};

use crate::model::RawChild;

/// what a reader needs besides the element: the part's text (for raw
/// copies), its relationships and the media table (part name to id).
pub struct ReadContext<'a> {
    pub part: &'a str,
    pub text: &'a str,
    pub rels: &'a Rels,
    pub media: &'a HashMap<String, String>,
}

impl ReadContext<'_> {
    pub fn raw(&self, element: &Element) -> String {
        self.text[element.start..element.end].to_string()
    }

    /// the media id a relationship id points at.
    pub fn media_id(&self, rel_id: &str) -> Option<String> {
        let rel = self.rels.get(rel_id)?;
        if rel.external {
            return None;
        }
        let part = rel.resolved(self.part);
        self.media
            .iter()
            .find(|(name, _)| name.eq_ignore_ascii_case(&part))
            .map(|(_, id)| id.clone())
    }
}

/// what a writer needs: the relationships of the part being written
/// (grown when a new picture is placed) and where media items live.
pub struct WriteContext<'a> {
    pub part: String,
    pub rels: Rels,
    /// media id to its part name in the package being written
    pub media_parts: &'a HashMap<String, String>,
}

impl WriteContext<'_> {
    pub fn media_rel(&mut self, media_id: &str, short_kind: &str) -> Option<String> {
        let target = self.media_parts.get(media_id)?.clone();
        Some(self.rels.ensure(&self.part.clone(), &target, short_kind))
    }
}

pub fn attr_i64(element: &Element, name: &str) -> Option<i64> {
    element
        .attr(name)
        .and_then(|value| value.trim().parse().ok())
}

pub fn attr_i32(element: &Element, name: &str) -> Option<i32> {
    element
        .attr(name)
        .and_then(|value| value.trim().parse().ok())
}

pub fn attr_bool(element: &Element, name: &str) -> Option<bool> {
    element
        .attr(name)
        .map(|value| matches!(value, "1" | "true" | "on"))
}

pub fn attr(name: &str, value: impl std::fmt::Display) -> String {
    format!(" {name}=\"{}\"", escape_attribute(&value.to_string()))
}

pub fn opt_attr<T: std::fmt::Display>(name: &str, value: Option<T>) -> String {
    value.map(|value| attr(name, value)).unwrap_or_default()
}

pub fn bool_text(value: bool) -> &'static str {
    if value {
        "1"
    } else {
        "0"
    }
}

/// the raw attributes in `extra` written back into a tag.
pub fn extra_attributes(extra: &[RawChild]) -> String {
    extra
        .iter()
        .filter(|child| !child.xml.starts_with('<'))
        .map(|child| format!(" {}", child.xml))
        .collect()
}

/// the raw children in `extra` that followed `after` (`None`: the ones
/// that came before any modelled child).
pub fn extra_after(extra: &[RawChild], after: Option<&str>) -> String {
    extra
        .iter()
        .filter(|child| child.xml.starts_with('<') && child.after.as_deref() == after)
        .map(|child| child.xml.as_str())
        .collect()
}

/// raw children that followed a modelled child the writer does not
/// write now; they go at the end so nothing is lost.
pub fn extra_orphans(extra: &[RawChild], written: &[&str]) -> String {
    extra
        .iter()
        .filter(|child| {
            child.xml.starts_with('<')
                && child
                    .after
                    .as_deref()
                    .is_some_and(|after| !written.contains(&after))
        })
        .map(|child| child.xml.as_str())
        .collect()
}

/// writes modelled children in schema order with the raw ones back where
/// they were: each slot is (the names that can fill it, its xml if it is
/// written). raw children recorded after any name of a slot follow that
/// slot even when the slot itself is not written now, so none is lost.
pub fn ordered_children(slots: &[(&[&str], Option<String>)], extra: &[RawChild]) -> String {
    let mut out = extra_after(extra, None);
    for (names, xml) in slots {
        if let Some(xml) = xml {
            out.push_str(xml);
        }
        for child in extra {
            if child.xml.starts_with('<')
                && child
                    .after
                    .as_deref()
                    .is_some_and(|after| names.contains(&after))
            {
                out.push_str(&child.xml);
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Geometry;
    use crate::xml::parse;

    fn context<'a>(
        text: &'a str,
        rels: &'a Rels,
        media: &'a HashMap<String, String>,
    ) -> ReadContext<'a> {
        ReadContext {
            part: "ppt/slides/slide1.xml",
            text,
            rels,
            media,
        }
    }

    #[test]
    fn colours_round_trip_with_transforms() {
        let text = r#"<a:solidFill xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main"><a:schemeClr val="accent1"><a:lumMod val="75000"/><a:lumOff val="25000"/></a:schemeClr></a:solidFill>"#;
        let document = parse(text).unwrap();
        let (rels, media) = (Rels::default(), HashMap::new());
        let fill = read_fill(&document.root, &context(text, &rels, &media)).unwrap();
        let media_parts = HashMap::new();
        let mut write = WriteContext {
            part: "x".into(),
            rels: Rels::default(),
            media_parts: &media_parts,
        };
        assert_eq!(
            write_fill(&fill, &mut write),
            r#"<a:solidFill><a:schemeClr val="accent1"><a:lumMod val="75000"/><a:lumOff val="25000"/></a:schemeClr></a:solidFill>"#
        );
    }

    #[test]
    fn preset_geometry_keeps_adjust_values() {
        let text = r#"<a:prstGeom xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" prst="roundRect"><a:avLst><a:gd name="adj" fmla="val 30000"/></a:avLst></a:prstGeom>"#;
        let document = parse(text).unwrap();
        let (rels, media) = (Rels::default(), HashMap::new());
        let geometry = read_geometry(&document.root, &context(text, &rels, &media)).unwrap();
        assert_eq!(
            geometry,
            Geometry::Preset {
                name: "roundRect".into(),
                adjust: vec![("adj".into(), 30000)]
            }
        );
        assert_eq!(
            write_geometry(&geometry),
            r#"<a:prstGeom prst="roundRect"><a:avLst><a:gd name="adj" fmla="val 30000"/></a:avLst></a:prstGeom>"#
        );
    }
}

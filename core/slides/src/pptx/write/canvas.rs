//! the part every slide, layout and master shares: `p:cSld` with its
//! background and shape tree, patched in place or written fresh.

use crate::xml::{self, Element};

use super::super::dml::{attr, write_color, write_fill, WriteContext};
use super::super::objects::write_objects;
use crate::model::{Background, Object};
use crate::preserve::{
    patch_element, rewrite_start_tag, start_tag_end, written_attributes, ChildEdit,
};

/// the namespaces every presentationml part declares
pub const NAMESPACES: &str = " xmlns:a=\"http://schemas.openxmlformats.org/drawingml/2006/main\" xmlns:r=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships\" xmlns:p=\"http://schemas.openxmlformats.org/presentationml/2006/main\"";

pub const XML_DECLARATION: &str =
    "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\r\n";

/// the shape tree's own properties, as powerpoint writes them
const TREE_PROPERTIES: &str = "<p:nvGrpSpPr><p:cNvPr id=\"1\" name=\"\"/><p:cNvGrpSpPr/><p:nvPr/></p:nvGrpSpPr><p:grpSpPr><a:xfrm><a:off x=\"0\" y=\"0\"/><a:ext cx=\"0\" cy=\"0\"/><a:chOff x=\"0\" y=\"0\"/><a:chExt cx=\"0\" cy=\"0\"/></a:xfrm></p:grpSpPr>";

/// schema order of `p:cSld` children
const COMMON_ORDER: [&str; 5] = [
    "p:bg",
    "p:spTree",
    "p:custDataLst",
    "p:controls",
    "p:extLst",
];

/// what goes into a `p:cSld`.
pub struct Canvas<'a> {
    pub name: Option<&'a str>,
    pub background: Option<&'a Background>,
    pub objects: &'a [Object],
}

pub fn background_xml(background: &Background, context: &mut WriteContext<'_>) -> String {
    if let Some(xml) = &background.xml {
        return xml.clone();
    }
    if let Some(fill) = &background.fill {
        return format!(
            "<p:bg><p:bgPr>{}<a:effectLst/></p:bgPr></p:bg>",
            write_fill(fill, context)
        );
    }
    match &background.style_ref {
        Some(reference) => {
            let color = reference
                .color
                .as_ref()
                .map(write_color)
                .unwrap_or_default();
            format!(
                "<p:bg><p:bgRef{}>{color}</p:bgRef></p:bg>",
                attr("idx", reference.index)
            )
        }
        None => String::new(),
    }
}

/// a fresh `p:cSld`.
pub fn fresh_canvas(canvas: &Canvas<'_>, context: &mut WriteContext<'_>) -> String {
    let name = canvas
        .name
        .map(|name| attr("name", name))
        .unwrap_or_default();
    let background = canvas
        .background
        .map(|b| background_xml(b, context))
        .unwrap_or_default();
    format!(
        "<p:cSld{name}>{background}<p:spTree>{TREE_PROPERTIES}{}</p:spTree></p:cSld>",
        write_objects(canvas.objects, context)
    )
}

/// the original part text with its `p:cSld` rebuilt: the shape tree's
/// own properties and extensions are kept, objects are written one by
/// one (unchanged ones as their original xml).
pub fn patched_part(
    text: &str,
    canvas: &Canvas<'_>,
    context: &mut WriteContext<'_>,
) -> Option<String> {
    let document = xml::parse(text).ok()?;
    let common = document.root.child("p:cSld")?;
    let tree = common.child("p:spTree")?;
    let new_tree = rebuilt_tree(text, tree, canvas.objects, context);
    let background_names = ["p:bg"];
    let tree_names = ["p:spTree"];
    let edits = [
        ChildEdit {
            names: &background_names,
            xml: canvas.background.map(|b| background_xml(b, context)),
        },
        ChildEdit {
            names: &tree_names,
            xml: Some(new_tree),
        },
    ];
    let tag = renamed_common_tag(text, common, canvas.name);
    let new_common = patch_element(text, common, tag.as_deref(), &edits, &COMMON_ORDER);
    let mut out = text.to_string();
    out.replace_range(common.start..common.end, &new_common);
    Some(out)
}

/// a new `p:cSld` opening tag when the slide's name changed.
fn renamed_common_tag(text: &str, common: &Element, name: Option<&str>) -> Option<String> {
    if common.attr("name") == name {
        return None;
    }
    let raw = &text[common.start..common.end];
    let self_closing = raw[..start_tag_end(raw)].ends_with("/>");
    let updates = [("name", name.map(str::to_string))];
    Some(rewrite_start_tag(
        "p:cSld",
        &written_attributes(raw),
        &updates,
        self_closing,
    ))
}

fn rebuilt_tree(
    text: &str,
    tree: &Element,
    objects: &[Object],
    context: &mut WriteContext<'_>,
) -> String {
    let raw = &text[tree.start..tree.end];
    let mut out = raw[..start_tag_end(raw)].to_string();
    for child in tree.elements() {
        if matches!(child.name.as_str(), "p:nvGrpSpPr" | "p:grpSpPr") {
            out.push_str(&text[child.start..child.end]);
        }
    }
    out.push_str(&write_objects(objects, context));
    for extensions in tree.children_named("p:extLst") {
        out.push_str(&text[extensions.start..extensions.end]);
    }
    out.push_str("</p:spTree>");
    out
}

/// the part text with its root opening tag's attributes changed, when
/// any of them really changed (otherwise the text is returned as is).
pub fn with_root_attributes(text: &str, updates: &[(&str, Option<String>)]) -> String {
    let Ok(document) = xml::parse(text) else {
        return text.to_string();
    };
    let root = &document.root;
    let differs = updates
        .iter()
        .any(|(name, value)| root.attr(name) != value.as_deref());
    if !differs {
        return text.to_string();
    }
    let raw = &text[root.start..root.end];
    let tag_end = start_tag_end(raw);
    let name = raw[1..]
        .split(|c: char| c.is_whitespace() || c == '>' || c == '/')
        .next()
        .unwrap_or("");
    let self_closing = raw[..tag_end].ends_with("/>");
    let tag = rewrite_start_tag(name, &written_attributes(raw), updates, self_closing);
    let mut out = text.to_string();
    out.replace_range(root.start..root.start + tag_end, &tag);
    out
}

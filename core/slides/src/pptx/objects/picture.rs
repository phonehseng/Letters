//! pictures, and videos and sounds with their poster frame (`p:pic`).

use crate::xml::Element;

use super::super::dml::{attr, attr_i32, ReadContext, WriteContext};
use super::super::rels::IMAGE;
use super::common::{
    application_properties, fresh_shape_properties, identity_tag, patched_non_visual,
    patched_shape_properties, read_identity, read_placeholder, read_shape_properties, retagged,
    Identity, ShapeProperties,
};
use super::{piece, Changes};
use crate::model::{Geometry, Picture};
use crate::preserve::{patch_element, unknown_children, ChildEdit};

const PICTURE_ORDER: [&str; 5] = ["p:nvPicPr", "p:blipFill", "p:spPr", "p:style", "p:extLst"];
const BLIP_FILL_ORDER: [&str; 4] = ["a:blip", "a:srcRect", "a:tile", "a:stretch"];

pub fn read_picture(element: &Element, context: &ReadContext<'_>) -> Picture {
    let nv = element.child("p:nvPicPr");
    let (id, name, description, hidden) = read_identity(nv);
    let blip_fill = element.child("p:blipFill");
    let blip = blip_fill.and_then(|fill| fill.child("a:blip"));
    let properties = read_shape_properties(element.child("p:spPr"), context);
    Picture {
        id,
        name,
        description,
        hidden,
        placeholder: read_placeholder(nv),
        transform: properties.transform,
        media_id: blip
            .and_then(|blip| blip.attr("r:embed"))
            .and_then(|rel| context.media_id(rel)),
        crop: blip_fill
            .and_then(|fill| fill.child("a:srcRect"))
            .map(read_crop),
        geometry: properties.geometry,
        line: properties.line,
        blip_extra: blip
            .map(|blip| unknown_children(context.text, blip, &[]))
            .unwrap_or_default(),
        media_kind: media_kind(nv),
        source: None,
    }
}

fn read_crop(rect: &Element) -> [i32; 4] {
    let side = |name: &str| attr_i32(rect, name).unwrap_or(0);
    [side("l"), side("t"), side("r"), side("b")]
}

/// "video" or "audio" when the picture is the poster of a media file.
fn media_kind(nv: Option<&Element>) -> Option<String> {
    let application = nv?.child("p:nvPr")?;
    if application.child("a:videoFile").is_some() {
        Some("video".to_string())
    } else if application.child("a:audioFile").is_some() {
        Some("audio".to_string())
    } else {
        None
    }
}

pub fn pieces(picture: &Picture) -> Vec<(String, String)> {
    let identity = (
        picture.id,
        &picture.name,
        &picture.description,
        picture.hidden,
        &picture.placeholder,
    );
    vec![
        piece("nv", &identity),
        piece(
            "blip",
            &(&picture.media_id, &picture.crop, &picture.blip_extra),
        ),
        piece("xfrm", &picture.transform),
        piece("geometry", &picture.geometry),
        piece("line", &picture.line),
    ]
}

fn identity(picture: &Picture) -> Identity<'_> {
    Identity {
        id: picture.id,
        name: &picture.name,
        description: &picture.description,
        hidden: picture.hidden,
    }
}

fn properties(picture: &Picture) -> ShapeProperties {
    ShapeProperties {
        transform: picture.transform.clone(),
        // a picture with no outline of its own is drawn in a rectangle
        geometry: picture
            .geometry
            .clone()
            .or_else(|| Some(Geometry::preset("rect"))),
        fill: None,
        line: picture.line.clone(),
    }
}

pub fn write_picture(
    picture: &Picture,
    original: Option<(&str, &Element)>,
    changes: &Changes<'_>,
    context: &mut WriteContext<'_>,
) -> String {
    match original {
        Some((source, element)) => patch_picture(picture, source, element, changes, context),
        None => fresh_picture(picture, context),
    }
}

fn crop_tag(crop: [i32; 4]) -> String {
    let side = |name: &str, value: i32| {
        if value != 0 {
            attr(name, value)
        } else {
            String::new()
        }
    };
    let [left, top, right, bottom] = crop;
    format!(
        "<a:srcRect{}{}{}{}/>",
        side("l", left),
        side("t", top),
        side("r", right),
        side("b", bottom)
    )
}

fn blip_fill(picture: &Picture, context: &mut WriteContext<'_>) -> String {
    let rel = picture
        .media_id
        .as_deref()
        .and_then(|id| context.media_rel(id, IMAGE));
    let extra: String = picture.blip_extra.iter().map(|c| c.xml.as_str()).collect();
    let mut out = String::from("<p:blipFill>");
    match rel {
        Some(rel) if extra.is_empty() => out.push_str(&format!("<a:blip r:embed=\"{rel}\"/>")),
        Some(rel) => out.push_str(&format!("<a:blip r:embed=\"{rel}\">{extra}</a:blip>")),
        None => out.push_str("<a:blip/>"),
    }
    if let Some(crop) = picture.crop {
        out.push_str(&crop_tag(crop));
    }
    out.push_str("<a:stretch><a:fillRect/></a:stretch></p:blipFill>");
    out
}

fn fresh_picture(picture: &Picture, context: &mut WriteContext<'_>) -> String {
    let mut out = String::from("<p:pic><p:nvPicPr>");
    out.push_str(&identity_tag(&identity(picture)));
    out.push_str("<p:cNvPicPr><a:picLocks noChangeAspect=\"1\"/></p:cNvPicPr>");
    out.push_str(&application_properties(&picture.placeholder));
    out.push_str("</p:nvPicPr>");
    out.push_str(&blip_fill(picture, context));
    out.push_str(&fresh_shape_properties(&properties(picture), &[], context));
    out.push_str("</p:pic>");
    out
}

fn patch_picture(
    picture: &Picture,
    source: &str,
    element: &Element,
    changes: &Changes<'_>,
    context: &mut WriteContext<'_>,
) -> String {
    let non_visual_names = ["p:nvPicPr"];
    let blip_names = ["p:blipFill"];
    let properties_names = ["p:spPr"];
    let mut edits = Vec::new();
    if changes.changed("nv") {
        if let Some(nv) = element.child("p:nvPicPr") {
            let xml =
                patched_non_visual(source, nv, &identity(picture), &picture.placeholder, None);
            edits.push(ChildEdit {
                names: &non_visual_names,
                xml: Some(xml),
            });
        }
    }
    if changes.changed("blip") {
        let xml = match element.child("p:blipFill") {
            Some(original) => patched_blip_fill(picture, source, original, context),
            None => blip_fill(picture, context),
        };
        edits.push(ChildEdit {
            names: &blip_names,
            xml: Some(xml),
        });
    }
    if changes.any(&["xfrm", "geometry", "line"]) {
        if let Some(properties_element) = element.child("p:spPr") {
            let mut properties = properties(picture);
            properties.geometry = picture.geometry.clone();
            let xml =
                patched_shape_properties(source, properties_element, &properties, changes, context);
            edits.push(ChildEdit {
                names: &properties_names,
                xml: Some(xml),
            });
        }
    }
    patch_element(source, element, None, &edits, &PICTURE_ORDER)
}

/// a picture's `p:blipFill` with a new picture or crop. a replaced
/// picture keeps its recolouring and tiling, which is what "replace
/// picture" promises.
fn patched_blip_fill(
    picture: &Picture,
    source: &str,
    original: &Element,
    context: &mut WriteContext<'_>,
) -> String {
    let blip_names = ["a:blip"];
    let rect_names = ["a:srcRect"];
    let rel = picture
        .media_id
        .as_deref()
        .and_then(|id| context.media_rel(id, IMAGE));
    let blip = original
        .child("a:blip")
        .map(|blip| retagged(source, blip, "a:blip", &[("r:embed", rel.clone())]));
    let edits = [
        ChildEdit {
            names: &blip_names,
            xml: blip,
        },
        ChildEdit {
            names: &rect_names,
            xml: picture.crop.map(crop_tag),
        },
    ];
    patch_element(source, original, None, &edits, &BLIP_FILL_ORDER)
}

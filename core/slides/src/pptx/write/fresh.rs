//! parts written from nothing: the package skeleton of a deck that did
//! not come from a pptx file, notes pages and the notes master.

use super::super::dml::WriteContext;
use super::super::rels::{self, Rels};
use super::canvas::{NAMESPACES, XML_DECLARATION};
use super::package::{OutPackage, PRESENTATION_PART};
use super::slides::notes_body_xml;
use crate::model::{Notes, Presentation};

pub const PRESENTATION_TYPE: &str =
    "application/vnd.openxmlformats-officedocument.presentationml.presentation.main+xml";
pub const NOTES_MASTER_TYPE: &str =
    "application/vnd.openxmlformats-officedocument.presentationml.notesMaster+xml";
const PROPERTIES_TYPE: &str =
    "application/vnd.openxmlformats-officedocument.presentationml.presProps+xml";
const VIEW_TYPE: &str =
    "application/vnd.openxmlformats-officedocument.presentationml.viewProps+xml";
const TABLE_STYLES_TYPE: &str =
    "application/vnd.openxmlformats-officedocument.presentationml.tableStyles+xml";

/// powerpoint's built-in "medium style 2, accent 1", the table style new
/// tables get; naming it as the default keeps new tables looking the same
const DEFAULT_TABLE_STYLE: &str = "{5C22544A-7EE6-4342-B048-85BDC9FD1C3A}";

/// the minimal package a deck needs before its own parts are written:
/// the root relationships, an empty presentation part and the three
/// small property parts powerpoint itself always writes.
pub fn add_package_skeleton(package: &mut OutPackage) {
    let mut root = Rels::default();
    root.ensure("", PRESENTATION_PART, rels::OFFICE_DOCUMENT);
    package.set_text("_rels/.rels", &root.to_xml());
    package.set_text(
        PRESENTATION_PART,
        &format!("{XML_DECLARATION}<p:presentation{NAMESPACES} saveSubsetFonts=\"1\"><p:sldMasterIdLst/><p:sldIdLst/><p:sldSz cx=\"12192000\" cy=\"6858000\"/><p:notesSz cx=\"6858000\" cy=\"9144000\"/><p:defaultTextStyle/></p:presentation>"),
    );
    let small_parts = [
        ("ppt/presProps.xml", PROPERTIES_TYPE, "presProps", format!("<p:presentationPr{NAMESPACES}/>")),
        ("ppt/viewProps.xml", VIEW_TYPE, "viewProps", format!("<p:viewPr{NAMESPACES}/>")),
        (
            "ppt/tableStyles.xml",
            TABLE_STYLES_TYPE,
            "tableStyles",
            format!("<a:tblStyleLst xmlns:a=\"http://schemas.openxmlformats.org/drawingml/2006/main\" def=\"{DEFAULT_TABLE_STYLE}\"/>"),
        ),
    ];
    let mut main_rels = Rels::default();
    for (part, content_type, kind, body) in small_parts {
        package.set_text(part, &format!("{XML_DECLARATION}{body}"));
        package.content_types.set_override(part, content_type);
        main_rels.ensure(PRESENTATION_PART, part, kind);
    }
    package.set_rels(PRESENTATION_PART, &main_rels);
    package.content_types.ensure_default(
        "rels",
        "application/vnd.openxmlformats-package.relationships+xml",
    );
    package
        .content_types
        .ensure_default("xml", "application/xml");
    package
        .content_types
        .set_override(PRESENTATION_PART, PRESENTATION_TYPE);
}

/// the deck's notes master part, made when there is none: notes pages
/// must point at one or powerpoint repairs the file.
pub fn ensure_notes_master(presentation: &Presentation, package: &mut OutPackage) -> String {
    if let Some(existing) = existing_notes_master(package) {
        return existing;
    }
    let part = "ppt/notesMasters/notesMaster1.xml".to_string();
    // the notes master gets its own copy of the first theme, as powerpoint does
    let theme_text = package
        .parts
        .iter()
        .find(|(name, _)| name.starts_with("ppt/theme/"))
        .map(|(_, bytes)| bytes.clone())
        .unwrap_or_default();
    let theme_part = package.numbered_name("ppt/theme/theme", &[]);
    package.set(&theme_part, theme_text);
    package
        .content_types
        .set_override(&theme_part, super::parts::THEME_TYPE);
    let mut master_rels = Rels::default();
    master_rels.ensure(&part, &theme_part, rels::THEME);
    package.set_rels(&part, &master_rels);
    package.set_text(&part, &notes_master_xml(presentation));
    package.content_types.set_override(&part, NOTES_MASTER_TYPE);
    part
}

fn existing_notes_master(package: &OutPackage) -> Option<String> {
    let main_rels = package.rels_of(PRESENTATION_PART);
    if let Some(rel) = main_rels.first_of(rels::NOTES_MASTER) {
        return Some(rel.resolved(PRESENTATION_PART));
    }
    package
        .parts
        .iter()
        .map(|(name, _)| name)
        .find(|name| name.starts_with("ppt/notesMasters/") && name.ends_with(".xml"))
        .cloned()
}

/// slide image and notes body placeholders on a portrait notes page, in
/// powerpoint's default positions for a 7.5 by 10 inch page
fn notes_master_xml(_presentation: &Presentation) -> String {
    format!(
        "{XML_DECLARATION}<p:notesMaster{NAMESPACES}><p:cSld><p:bg><p:bgRef idx=\"1001\"><a:schemeClr val=\"bg1\"/></p:bgRef></p:bg><p:spTree><p:nvGrpSpPr><p:cNvPr id=\"1\" name=\"\"/><p:cNvGrpSpPr/><p:nvPr/></p:nvGrpSpPr><p:grpSpPr><a:xfrm><a:off x=\"0\" y=\"0\"/><a:ext cx=\"0\" cy=\"0\"/><a:chOff x=\"0\" y=\"0\"/><a:chExt cx=\"0\" cy=\"0\"/></a:xfrm></p:grpSpPr>{}{}</p:spTree></p:cSld><p:clrMap bg1=\"lt1\" tx1=\"dk1\" bg2=\"lt2\" tx2=\"dk2\" accent1=\"accent1\" accent2=\"accent2\" accent3=\"accent3\" accent4=\"accent4\" accent5=\"accent5\" accent6=\"accent6\" hlink=\"hlink\" folHlink=\"folHlink\"/><p:notesStyle><a:lvl1pPr marL=\"0\" algn=\"l\" defTabSz=\"914400\" rtl=\"0\" eaLnBrk=\"1\" latinLnBrk=\"0\" hangingPunct=\"1\"><a:defRPr sz=\"1200\" kern=\"1200\"><a:solidFill><a:schemeClr val=\"tx1\"/></a:solidFill><a:latin typeface=\"+mn-lt\"/><a:ea typeface=\"+mn-ea\"/><a:cs typeface=\"+mn-cs\"/></a:defRPr></a:lvl1pPr></p:notesStyle></p:notesMaster>",
        "<p:sp><p:nvSpPr><p:cNvPr id=\"2\" name=\"Slide Image Placeholder 1\"/><p:cNvSpPr><a:spLocks noGrp=\"1\" noRot=\"1\" noChangeAspect=\"1\"/></p:cNvSpPr><p:nvPr><p:ph type=\"sldImg\" idx=\"2\"/></p:nvPr></p:nvSpPr><p:spPr><a:xfrm><a:off x=\"685800\" y=\"1143000\"/><a:ext cx=\"5486400\" cy=\"3086100\"/></a:xfrm><a:prstGeom prst=\"rect\"><a:avLst/></a:prstGeom><a:noFill/><a:ln w=\"12700\"><a:solidFill><a:prstClr val=\"black\"/></a:solidFill></a:ln></p:spPr></p:sp>",
        "<p:sp><p:nvSpPr><p:cNvPr id=\"3\" name=\"Notes Placeholder 2\"/><p:cNvSpPr><a:spLocks noGrp=\"1\"/></p:cNvSpPr><p:nvPr><p:ph type=\"body\" sz=\"quarter\" idx=\"3\"/></p:nvPr></p:nvSpPr><p:spPr><a:xfrm><a:off x=\"685800\" y=\"4400550\"/><a:ext cx=\"5486400\" cy=\"3600450\"/></a:xfrm><a:prstGeom prst=\"rect\"><a:avLst/></a:prstGeom></p:spPr><p:txBody><a:bodyPr vert=\"horz\" lIns=\"91440\" tIns=\"45720\" rIns=\"91440\" bIns=\"45720\" rtlCol=\"0\"/><a:lstStyle/><a:p><a:endParaRPr lang=\"en-US\"/></a:p></p:txBody></p:sp>"
    )
}

/// a notes page: the slide image and the notes text placeholders.
pub fn fresh_notes(notes: &Notes, context: &mut WriteContext<'_>) -> String {
    format!(
        "{XML_DECLARATION}<p:notes{NAMESPACES}><p:cSld><p:spTree><p:nvGrpSpPr><p:cNvPr id=\"1\" name=\"\"/><p:cNvGrpSpPr/><p:nvPr/></p:nvGrpSpPr><p:grpSpPr><a:xfrm><a:off x=\"0\" y=\"0\"/><a:ext cx=\"0\" cy=\"0\"/><a:chOff x=\"0\" y=\"0\"/><a:chExt cx=\"0\" cy=\"0\"/></a:xfrm></p:grpSpPr><p:sp><p:nvSpPr><p:cNvPr id=\"2\" name=\"Slide Image Placeholder 1\"/><p:cNvSpPr><a:spLocks noGrp=\"1\" noRot=\"1\" noChangeAspect=\"1\"/></p:cNvSpPr><p:nvPr><p:ph type=\"sldImg\"/></p:nvPr></p:nvSpPr><p:spPr/></p:sp><p:sp><p:nvSpPr><p:cNvPr id=\"3\" name=\"Notes Placeholder 2\"/><p:cNvSpPr><a:spLocks noGrp=\"1\"/></p:cNvSpPr><p:nvPr><p:ph type=\"body\" idx=\"1\"/></p:nvPr></p:nvSpPr><p:spPr/>{}</p:sp></p:spTree></p:cSld><p:clrMapOvr><a:masterClrMapping/></p:clrMapOvr></p:notes>",
        notes_body_xml(notes, context)
    )
}

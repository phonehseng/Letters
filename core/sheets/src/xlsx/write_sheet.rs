//! one worksheet part, written in the order the schema requires
//! (ecma-376 part 1, CT_Worksheet). elements the model makes are
//! compared with what it made when the file was read: unchanged ones are
//! written as their original xml. elements the model does not read are
//! written as they were, in their place.

use crate::model::carry::{fingerprint, Section};
use crate::model::Sheet;

use super::read_sheet::MODELLED;
use super::write_cells::{write_sheet_data, StringTable};
use super::write_sections as make;

/// every worksheet child in schema order
pub const ORDER: [&str; 39] = [
    "sheetPr",
    "dimension",
    "sheetViews",
    "sheetFormatPr",
    "cols",
    "sheetData",
    "sheetCalcPr",
    "sheetProtection",
    "protectedRanges",
    "scenarios",
    "autoFilter",
    "sortState",
    "dataConsolidate",
    "customSheetViews",
    "mergeCells",
    "phoneticPr",
    "conditionalFormatting",
    "dataValidations",
    "hyperlinks",
    "printOptions",
    "pageMargins",
    "pageSetup",
    "headerFooter",
    "rowBreaks",
    "colBreaks",
    "customProperties",
    "cellWatches",
    "ignoredErrors",
    "smartTags",
    "drawing",
    "legacyDrawing",
    "legacyDrawingHF",
    "drawingHF",
    "picture",
    "oleObjects",
    "controls",
    "webPublishItems",
    "tableParts",
    "extLst",
];

/// relationship ids the worksheet's elements refer to
#[derive(Default)]
pub struct SheetIds {
    pub hyperlinks: Vec<Option<String>>,
    pub legacy_drawing: Option<String>,
    pub tables: Vec<String>,
}

/// what the model makes for one modelled element
pub fn generate(name: &str, sheet: &Sheet, ids: &SheetIds) -> Option<String> {
    match name {
        "sheetPr" => make::sheet_properties(sheet),
        "dimension" => Some(make::dimension(sheet)),
        "sheetViews" => make::sheet_views(sheet),
        "sheetFormatPr" => make::format_properties(sheet),
        "cols" => make::columns(sheet),
        "autoFilter" => sheet.auto_filter.as_ref().map(make::auto_filter),
        "mergeCells" => make::merges(sheet),
        "conditionalFormatting" => make::conditional_formats(sheet),
        "dataValidations" => make::validations(sheet),
        "hyperlinks" => make::hyperlinks(sheet, &ids.hyperlinks),
        "printOptions" => make::print_options(sheet),
        "pageMargins" => make::margins(sheet),
        "pageSetup" => make::page_setup(sheet),
        "headerFooter" => make::header_footer(sheet),
        "rowBreaks" => make::breaks(
            "rowBreaks",
            &sheet.print.row_breaks,
            crate::address::MAX_COLS - 1,
        ),
        "colBreaks" => make::breaks(
            "colBreaks",
            &sheet.print.col_breaks,
            crate::address::MAX_ROWS - 1,
        ),
        "legacyDrawing" => ids
            .legacy_drawing
            .as_ref()
            .map(|id| format!("<legacyDrawing r:id=\"{id}\"/>")),
        "tableParts" => table_parts(&ids.tables),
        _ => None,
    }
}

fn table_parts(ids: &[String]) -> Option<String> {
    if ids.is_empty() {
        return None;
    }
    let parts: String = ids
        .iter()
        .map(|id| format!("<tablePart r:id=\"{id}\"/>"))
        .collect();
    Some(format!(
        "<tableParts count=\"{}\">{parts}</tableParts>",
        ids.len()
    ))
}

/// elements whose relationship ids are made fresh on every save, so
/// their original xml is never reused
const ALWAYS_REMADE: [&str; 3] = ["dimension", "legacyDrawing", "tableParts"];

/// right after reading: notes what the model makes of each element, so
/// a save can tell whether anything changed
pub fn remember_generated(sheet: &mut Sheet, ids: &SheetIds) {
    let mut sections = std::mem::take(&mut sheet.carry.sections);
    for section in sections.iter_mut() {
        let modelled = MODELLED.contains(&section.name.as_str());
        if modelled && !ALWAYS_REMADE.contains(&section.name.as_str()) {
            let made = generate(&section.name, sheet, ids).unwrap_or_default();
            section.generated = Some(fingerprint(&made));
        }
    }
    sheet.carry.sections = sections;
}

/// the xml for one element name: the original when unchanged, else the
/// model's, else whatever was kept
fn element_xml(name: &str, sheet: &Sheet, ids: &SheetIds) -> Option<String> {
    let kept = sheet
        .carry
        .sections
        .iter()
        .find(|section| section.name == name);
    if !MODELLED.contains(&name) {
        return kept.map(|section| section.xml.clone());
    }
    let made = generate(name, sheet, ids);
    if let Some(section) = kept {
        let unchanged = section.generated == Some(fingerprint(made.as_deref().unwrap_or("")));
        if unchanged && !ALWAYS_REMADE.contains(&name) {
            return Some(section.xml.clone());
        }
    }
    made
}

/// kept elements with names outside the schema list (markup compatibility
/// wrappers, for instance), grouped by the known element they followed
fn strays_after(sections: &[Section]) -> Vec<(&str, &Section)> {
    let mut strays = Vec::new();
    let mut previous = "";
    for section in sections {
        if ORDER.contains(&section.name.as_str()) {
            previous = ORDER
                .iter()
                .find(|name| **name == section.name)
                .copied()
                .unwrap_or("");
        } else {
            strays.push((previous, section));
        }
    }
    strays
}

const MAIN_NS: &str = "http://schemas.openxmlformats.org/spreadsheetml/2006/main";
const RELATIONSHIPS_NS: &str =
    "http://schemas.openxmlformats.org/officeDocument/2006/relationships";

fn root_attributes(sheet: &Sheet) -> Vec<(String, String)> {
    let mut attributes = sheet.carry.root_attributes.clone();
    if !attributes.iter().any(|(key, _)| key == "xmlns") {
        attributes.insert(0, ("xmlns".into(), MAIN_NS.into()));
    }
    if !attributes.iter().any(|(key, _)| key == "xmlns:r") {
        attributes.push(("xmlns:r".into(), RELATIONSHIPS_NS.into()));
    }
    attributes
}

pub fn write_worksheet(sheet: &Sheet, ids: &SheetIds, strings: &mut StringTable) -> String {
    let mut out =
        String::from("<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<worksheet");
    for (key, value) in root_attributes(sheet) {
        out.push_str(&format!(
            " {key}=\"{}\"",
            crate::xml::escape_attribute(&value)
        ));
    }
    out.push('>');
    let strays = strays_after(&sheet.carry.sections);
    for (stray_after, section) in &strays {
        if stray_after.is_empty() {
            out.push_str(&section.xml);
        }
    }
    for name in ORDER {
        if name == "sheetData" {
            write_sheet_data(sheet, strings, &mut out);
        } else if let Some(xml) = element_xml(name, sheet, ids) {
            out.push_str(&xml);
        }
        for (stray_after, section) in &strays {
            if *stray_after == name {
                out.push_str(&section.xml);
            }
        }
    }
    out.push_str("</worksheet>");
    out
}

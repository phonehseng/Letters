//! the workbook part (xl/workbook.xml) and the core document
//! properties, written from the model with the elements kept from the
//! file in schema order (ecma-376 part 1, CT_Workbook).

use crate::model::*;
use crate::xml::{escape_attribute, escape_text, Fragment};

/// workbook children in schema order (ecma-376 part 1, CT_Workbook)
const WORKBOOK_ORDER: [&str; 19] = [
    "fileVersion",
    "fileSharing",
    "workbookPr",
    "workbookProtection",
    "bookViews",
    "sheets",
    "functionGroups",
    "externalReferences",
    "definedNames",
    "calcPr",
    "oleSize",
    "customWorkbookViews",
    "pivotCaches",
    "smartTagPr",
    "smartTagTypes",
    "webPublishing",
    "fileRecoveryPr",
    "webPublishObjects",
    "extLst",
];

fn kept_section<'a>(workbook: &'a Workbook, name: &str) -> Option<&'a str> {
    workbook
        .carry
        .workbook_sections
        .iter()
        .find(|section| section.name == name)
        .map(|section| section.xml.as_str())
}

pub(super) fn workbook_part_xml(workbook: &Workbook, sheet_ids: &[String]) -> String {
    let mut out =
        String::from("<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<workbook");
    let mut attributes = workbook.carry.workbook_root_attributes.clone();
    if attributes.is_empty() {
        attributes.push((
            "xmlns".into(),
            "http://schemas.openxmlformats.org/spreadsheetml/2006/main".into(),
        ));
    }
    if !attributes.iter().any(|(key, _)| key == "xmlns:r") {
        attributes.push((
            "xmlns:r".into(),
            "http://schemas.openxmlformats.org/officeDocument/2006/relationships".into(),
        ));
    }
    for (key, value) in &attributes {
        out.push_str(&format!(" {key}=\"{}\"", escape_attribute(value)));
    }
    out.push('>');
    for name in WORKBOOK_ORDER {
        let xml = match name {
            "workbookPr" => Some(workbook_properties(workbook)),
            "bookViews" => Some(book_views(workbook)),
            "sheets" => Some(sheets_xml(workbook, sheet_ids)),
            "definedNames" => defined_names_xml(workbook),
            "calcPr" => Some(calc_properties(workbook)),
            other => kept_section(workbook, other).map(str::to_string),
        };
        if let Some(xml) = xml {
            out.push_str(&xml);
        }
    }
    out.push_str("</workbook>");
    out
}

fn workbook_properties(workbook: &Workbook) -> String {
    let mut fragment = kept_section(workbook, "workbookPr")
        .and_then(Fragment::parse)
        .unwrap_or_else(|| Fragment {
            name: "workbookPr".into(),
            ..Fragment::default()
        });
    let date1904 = (workbook.date_system == DateSystem::Excel1904).then(|| "1".to_string());
    fragment.set_attribute("date1904", date1904);
    fragment.to_xml()
}

fn book_views(workbook: &Workbook) -> String {
    let active = workbook.active_sheet.to_string();
    let Some(mut views) = kept_section(workbook, "bookViews").and_then(Fragment::parse) else {
        return format!("<bookViews><workbookView activeTab=\"{active}\"/></bookViews>");
    };
    if let Some((_, first)) = views.children.first_mut() {
        if let Some(mut view) = Fragment::parse(first) {
            let tab = (workbook.active_sheet != 0).then_some(active);
            view.set_attribute("activeTab", tab);
            *first = view.to_xml();
        }
    }
    views.to_xml()
}

fn sheets_xml(workbook: &Workbook, sheet_ids: &[String]) -> String {
    let mut used_ids: Vec<u32> = workbook
        .sheets
        .iter()
        .filter_map(|s| s.carry.sheet_id)
        .collect();
    let mut out = String::from("<sheets>");
    for (sheet, relationship) in workbook.sheets.iter().zip(sheet_ids) {
        let id = sheet.carry.sheet_id.unwrap_or_else(|| {
            let next = used_ids.iter().max().copied().unwrap_or(0) + 1;
            used_ids.push(next);
            next
        });
        let state = match sheet.visibility {
            Visibility::Visible => "",
            Visibility::Hidden => " state=\"hidden\"",
            Visibility::VeryHidden => " state=\"veryHidden\"",
        };
        out.push_str(&format!(
            "<sheet name=\"{}\" sheetId=\"{id}\"{state} r:id=\"{relationship}\"/>",
            escape_attribute(&sheet.name)
        ));
    }
    out.push_str("</sheets>");
    out
}

/// the workbook's names plus each sheet's print area and titles, which
/// excel stores as hidden names
fn defined_names_xml(workbook: &Workbook) -> Option<String> {
    let mut names = workbook.defined_names.clone();
    for (index, sheet) in workbook.sheets.iter().enumerate() {
        names.extend(print_names(index, sheet));
    }
    if names.is_empty() {
        return None;
    }
    let mut out = String::from("<definedNames>");
    for name in &names {
        out.push_str(&format!(
            "<definedName name=\"{}\"",
            escape_attribute(&name.name)
        ));
        if let Some(sheet) = name.sheet {
            out.push_str(&format!(" localSheetId=\"{sheet}\""));
        }
        if name.hidden {
            out.push_str(" hidden=\"1\"");
        }
        if let Some(comment) = &name.comment {
            out.push_str(&format!(" comment=\"{}\"", escape_attribute(comment)));
        }
        for (key, value) in &name.attributes {
            out.push_str(&format!(" {key}=\"{}\"", escape_attribute(value)));
        }
        let formula = super::write_cells::file_formula_text(&name.formula);
        out.push_str(&format!(">{}</definedName>", escape_text(&formula)));
    }
    out.push_str("</definedNames>");
    Some(out)
}

fn absolute_range(range: &crate::address::Range) -> String {
    let text = range.to_a1();
    // "A1:C10" -> "$A$1:$C$10"; whole rows and columns get "$" before each part
    text.split(':')
        .map(|part| {
            let letters: String = part
                .chars()
                .take_while(|c| c.is_ascii_alphabetic())
                .collect();
            let digits = &part[letters.len()..];
            match (letters.is_empty(), digits.is_empty()) {
                (false, false) => format!("${letters}${digits}"),
                _ => format!("${part}"),
            }
        })
        .collect::<Vec<_>>()
        .join(":")
}

fn print_names(index: usize, sheet: &Sheet) -> Vec<DefinedName> {
    let prefix = crate::formula::print::quote_sheet(&sheet.name);
    let mut names = Vec::new();
    if !sheet.print.area.is_empty() {
        let parts: Vec<String> = sheet
            .print
            .area
            .iter()
            .map(|r| format!("{prefix}!{}", absolute_range(r)))
            .collect();
        names.push(DefinedName {
            name: "_xlnm.Print_Area".into(),
            formula: parts.join(","),
            sheet: Some(index),
            ..DefinedName::default()
        });
    }
    let mut titles = Vec::new();
    if let Some((first, last)) = sheet.print.title_cols {
        titles.push(format!(
            "{prefix}!${}:${}",
            crate::address::column_name(first),
            crate::address::column_name(last)
        ));
    }
    if let Some((first, last)) = sheet.print.title_rows {
        titles.push(format!("{prefix}!${}:${}", first + 1, last + 1));
    }
    if !titles.is_empty() {
        names.push(DefinedName {
            name: "_xlnm.Print_Titles".into(),
            formula: titles.join(","),
            sheet: Some(index),
            ..DefinedName::default()
        });
    }
    names
}

fn calc_properties(workbook: &Workbook) -> String {
    let calc = &workbook.calculation;
    let mut fragment = kept_section(workbook, "calcPr")
        .and_then(Fragment::parse)
        .unwrap_or_else(|| Fragment {
            name: "calcPr".into(),
            ..Fragment::default()
        });
    fragment.set_attribute("calcId", calc.calc_id.clone().or(Some("191029".into())));
    fragment.set_attribute("calcMode", (!calc.automatic).then(|| "manual".to_string()));
    fragment.set_attribute("iterate", calc.iterate.then(|| "1".to_string()));
    fragment.set_attribute(
        "fullCalcOnLoad",
        calc.full_calc_on_load.then(|| "1".to_string()),
    );
    fragment.to_xml()
}

/// the core document properties part
pub fn core_properties_xml(metadata: &Metadata) -> String {
    let mut out = String::from(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<cp:coreProperties xmlns:cp=\"http://schemas.openxmlformats.org/package/2006/metadata/core-properties\" xmlns:dc=\"http://purl.org/dc/elements/1.1/\" xmlns:dcterms=\"http://purl.org/dc/terms/\" xmlns:dcmitype=\"http://purl.org/dc/dcmitype/\" xmlns:xsi=\"http://www.w3.org/2001/XMLSchema-instance\">",
    );
    let texts = [
        ("dc:title", &metadata.title),
        ("dc:subject", &metadata.subject),
        ("dc:creator", &metadata.creator),
        ("cp:keywords", &metadata.keywords),
        ("dc:description", &metadata.description),
        ("cp:lastModifiedBy", &metadata.last_modified_by),
    ];
    for (name, value) in texts {
        if let Some(value) = value {
            out.push_str(&format!("<{name}>{}</{name}>", escape_text(value)));
        }
    }
    let dates = [
        ("dcterms:created", &metadata.created),
        ("dcterms:modified", &metadata.modified),
    ];
    for (name, value) in dates {
        if let Some(value) = value {
            out.push_str(&format!(
                "<{name} xsi:type=\"dcterms:W3CDTF\">{}</{name}>",
                escape_text(value)
            ));
        }
    }
    out.push_str("</cp:coreProperties>");
    out
}

//! reading a whole xlsx package: the workbook part, styles, shared
//! strings, theme colours, document properties and every worksheet with
//! the parts it points to. whatever is not read is carried byte for byte.

use std::collections::HashSet;

use crate::error::SheetError;
use crate::model::carry::{fingerprint, CarriedPart, Relationship, Section, SourceFormat};
use crate::model::*;
use crate::package::{resolve_target, Package};
use crate::xml::{Event, Fragment, Reader};

use super::read_cells::{read_shared_strings, CellContext, SharedString};
use super::read_sheet::read_worksheet;
use super::read_sheet_parts::read_sheet_relations;
use super::relationships::*;
use super::write_sheet::{remember_generated, SheetIds};

/// one `<sheet>` entry of the workbook part
struct SheetEntry {
    name: String,
    sheet_id: Option<u32>,
    state: Visibility,
    relationship: String,
}

/// the open package and what has been consumed from it
pub(super) struct ReadState {
    pub(super) package: Package,
    used: HashSet<String>,
}

impl ReadState {
    pub(super) fn text(&mut self, part: &str) -> Result<Option<String>, SheetError> {
        let text = self.package.text(part)?;
        if text.is_some() {
            self.used.insert(part.to_ascii_lowercase());
        }
        Ok(text)
    }

    pub(super) fn relationships_of(&mut self, part: &str) -> Vec<Relationship> {
        let rels = rels_part_for(part);
        match self.text(&rels) {
            Ok(Some(xml)) => read_relationships(&xml),
            _ => Vec::new(),
        }
    }
}

pub fn read_xlsx(bytes: &[u8]) -> Result<Workbook, SheetError> {
    let package = Package::read(bytes)?;
    let mut state = ReadState {
        package,
        used: HashSet::new(),
    };
    let mut workbook = Workbook::new();
    workbook.sheets.clear();
    workbook.carry.source = Some(SourceFormat::Xlsx);
    let types = state
        .text("[Content_Types].xml")?
        .map(|xml| read_content_types(&xml))
        .unwrap_or_default();
    let root_rels = state
        .text("_rels/.rels")?
        .map(|xml| read_relationships(&xml))
        .unwrap_or_default();
    let main = root_rels
        .iter()
        .find(|rel| rel.kind == OFFICE_DOCUMENT_REL)
        .map(|rel| resolve_target("", &rel.target))
        .unwrap_or_else(|| "xl/workbook.xml".to_string());
    workbook.carry.main_content_type = types
        .iter()
        .find(|t| t.key.eq_ignore_ascii_case(&main))
        .map(|t| t.content_type.clone());
    workbook.has_macros = workbook
        .carry
        .main_content_type
        .as_deref()
        .is_some_and(|t| t.contains("macroEnabled"));
    workbook.carry.package_relationships = root_rels.clone();
    read_core_properties(&mut state, &root_rels, &mut workbook)?;
    let workbook_rels = state.relationships_of(&main);
    let xml = state
        .text(&main)?
        .ok_or_else(|| SheetError::NotASpreadsheet("the workbook part is missing".into()))?;
    let entries = read_workbook_part(&xml, &mut workbook)
        .map_err(|detail| SheetError::bad_xml(&main, detail))?;
    read_styles_part(&mut state, &main, &workbook_rels, &mut workbook)?;
    let strings = read_strings_part(&mut state, &main, &workbook_rels)?;
    read_sheets(
        &mut state,
        &main,
        &workbook_rels,
        &entries,
        &strings,
        &mut workbook,
    )?;
    move_print_names_to_sheets(&mut workbook);
    workbook.carry.workbook_relationships = workbook_rels
        .into_iter()
        .filter(|rel| {
            ![
                WORKSHEET_REL,
                STYLES_REL,
                SHARED_STRINGS_REL,
                CALC_CHAIN_REL,
            ]
            .contains(&rel.kind.as_str())
        })
        .collect();
    carry_remaining_parts(&mut state, &types, &mut workbook);
    if workbook.sheets.is_empty() {
        workbook.sheets.push(Sheet::new("Sheet1"));
    }
    workbook.active_sheet = workbook.active_sheet.min(workbook.sheets.len() - 1);
    Ok(workbook)
}

fn target_of(rels: &[Relationship], kind: &str, source: &str) -> Option<String> {
    rels.iter()
        .find(|rel| rel.kind == kind)
        .map(|rel| resolve_target(source, &rel.target))
}

fn read_workbook_part(xml: &str, workbook: &mut Workbook) -> Result<Vec<SheetEntry>, String> {
    let mut reader = Reader::new(xml);
    let root = reader.root()?;
    workbook.carry.workbook_root_attributes = root.owned_attributes();
    let mut entries = Vec::new();
    loop {
        let tag = match reader.next()? {
            Some(Event::Start(tag)) => tag,
            Some(Event::End { .. }) | None => break,
            Some(Event::Text(_)) => continue,
        };
        let name = tag.local().to_string();
        let raw = reader.skip_element(&tag)?;
        match name.as_str() {
            "workbookPr" => read_workbook_properties(raw, workbook),
            "bookViews" => read_book_views(raw, workbook),
            "sheets" => entries = read_sheet_entries(raw),
            "definedNames" => workbook.defined_names = read_defined_names(raw),
            "calcPr" => read_calc_properties(raw, workbook),
            _ => {}
        }
        workbook.carry.workbook_sections.push(Section {
            name,
            xml: raw.to_string(),
            generated: None,
        });
    }
    Ok(entries)
}

fn read_workbook_properties(raw: &str, workbook: &mut Workbook) {
    let Some(fragment) = Fragment::parse(raw) else {
        return;
    };
    if matches!(fragment.attribute("date1904"), Some("1") | Some("true")) {
        workbook.date_system = DateSystem::Excel1904;
    }
}

fn read_book_views(raw: &str, workbook: &mut Workbook) {
    let Some(fragment) = Fragment::parse(raw) else {
        return;
    };
    let first = fragment
        .children
        .first()
        .and_then(|(_, xml)| Fragment::parse(xml));
    workbook.active_sheet = first
        .and_then(|view| view.attribute("activeTab")?.parse().ok())
        .unwrap_or(0);
}

fn read_sheet_entries(raw: &str) -> Vec<SheetEntry> {
    let Some(fragment) = Fragment::parse(raw) else {
        return Vec::new();
    };
    fragment
        .children
        .iter()
        .filter_map(|(_, xml)| {
            let sheet = Fragment::parse(xml)?;
            Some(SheetEntry {
                name: sheet.attribute("name")?.to_string(),
                sheet_id: sheet.attribute("sheetId").and_then(|id| id.parse().ok()),
                state: match sheet.attribute("state") {
                    Some("hidden") => Visibility::Hidden,
                    Some("veryHidden") => Visibility::VeryHidden,
                    _ => Visibility::Visible,
                },
                relationship: sheet.attribute("r:id")?.to_string(),
            })
        })
        .collect()
}

/// defined name attributes the model reads
const NAME_ATTRIBUTES: [&str; 4] = ["name", "localSheetId", "hidden", "comment"];

fn read_defined_names(raw: &str) -> Vec<DefinedName> {
    let Some(fragment) = Fragment::parse(raw) else {
        return Vec::new();
    };
    fragment
        .children
        .iter()
        .filter_map(|(_, xml)| {
            let name = Fragment::parse(xml)?;
            let formula = super::read_sections::element_text(xml).unwrap_or_default();
            Some(DefinedName {
                name: name.attribute("name")?.to_string(),
                formula: super::read_cells::canonical_formula_text(&formula),
                sheet: name
                    .attribute("localSheetId")
                    .and_then(|id| id.parse().ok()),
                hidden: name.attribute("hidden") == Some("1"),
                comment: name.attribute("comment").map(str::to_string),
                attributes: name
                    .attributes
                    .iter()
                    .filter(|(key, _)| !NAME_ATTRIBUTES.contains(&key.as_str()))
                    .cloned()
                    .collect(),
            })
        })
        .collect()
}

fn read_calc_properties(raw: &str, workbook: &mut Workbook) {
    let Some(fragment) = Fragment::parse(raw) else {
        return;
    };
    let calc = &mut workbook.calculation;
    calc.automatic = fragment.attribute("calcMode") != Some("manual");
    calc.iterate = fragment.attribute("iterate") == Some("1");
    calc.iterate_count = fragment
        .attribute("iterateCount")
        .and_then(|v| v.parse().ok())
        .unwrap_or(100);
    calc.iterate_delta = fragment
        .attribute("iterateDelta")
        .and_then(|v| v.parse().ok())
        .unwrap_or(0.001);
    calc.full_calc_on_load = fragment.attribute("fullCalcOnLoad") == Some("1");
    calc.calc_id = fragment.attribute("calcId").map(str::to_string);
}

fn read_styles_part(
    state: &mut ReadState,
    main: &str,
    rels: &[Relationship],
    workbook: &mut Workbook,
) -> Result<(), SheetError> {
    if let Some(theme) = target_of(rels, THEME_REL, main) {
        if let Some(xml) = state.package.text(&theme)? {
            let colors = read_theme_colors(&xml);
            if colors.len() >= 12 {
                workbook.styles.theme_colors = colors;
            }
        }
    }
    let Some(part) = target_of(rels, STYLES_REL, main) else {
        return Ok(());
    };
    let Some(xml) = state.text(&part)? else {
        return Ok(());
    };
    let read =
        super::styles::read_styles(&xml).map_err(|detail| SheetError::bad_xml(&part, detail))?;
    let theme = std::mem::take(&mut workbook.styles.theme_colors);
    workbook.styles = read.table;
    workbook.styles.theme_colors = theme;
    workbook.carry.styles_sections = read.sections;
    workbook.carry.styles_root_attributes = read.root_attributes;
    Ok(())
}

/// the theme's colour scheme, in the index order cells use (light 1 and
/// dark 1 swapped, as excel does; see `StyleTable::theme_colors`)
fn read_theme_colors(xml: &str) -> Vec<String> {
    let mut reader = Reader::new(xml);
    let mut in_scheme = false;
    let mut colors = Vec::new();
    while let Ok(Some(event)) = reader.next() {
        match event {
            Event::Start(tag) if tag.local() == "clrScheme" => in_scheme = true,
            Event::Start(tag) if in_scheme => {
                let value = tag.attr("lastClr").or_else(|| tag.attr("val"));
                if matches!(tag.local(), "srgbClr" | "sysClr") {
                    colors.extend(value.map(|v| v.to_ascii_uppercase()));
                }
            }
            Event::End {
                local: "clrScheme", ..
            } => break,
            _ => {}
        }
    }
    if colors.len() >= 4 {
        colors.swap(0, 1);
        colors.swap(2, 3);
    }
    colors
}

fn read_strings_part(
    state: &mut ReadState,
    main: &str,
    rels: &[Relationship],
) -> Result<Vec<SharedString>, SheetError> {
    let Some(part) = target_of(rels, SHARED_STRINGS_REL, main) else {
        return Ok(Vec::new());
    };
    let Some(xml) = state.text(&part)? else {
        return Ok(Vec::new());
    };
    read_shared_strings(&xml).map_err(|detail| SheetError::bad_xml(&part, detail))
}

fn read_sheets(
    state: &mut ReadState,
    main: &str,
    rels: &[Relationship],
    entries: &[SheetEntry],
    strings: &[SharedString],
    workbook: &mut Workbook,
) -> Result<(), SheetError> {
    let context = CellContext {
        strings,
        date_system: workbook.date_system,
        style_count: workbook.styles.styles.len() as u32,
    };
    for entry in entries {
        let Some(rel) = rels.iter().find(|rel| rel.id == entry.relationship) else {
            continue;
        };
        if rel.kind != WORKSHEET_REL {
            // chart sheets and dialog sheets stay as carried parts
            continue;
        }
        let part = resolve_target(main, &rel.target);
        let Some(xml) = state.text(&part)? else {
            continue;
        };
        let mut sheet = Sheet::new(entry.name.clone());
        sheet.visibility = entry.state;
        sheet.carry.source = Some(SourceFormat::Xlsx);
        sheet.carry.part = Some(part.clone());
        sheet.carry.sheet_id = entry.sheet_id;
        let links = read_worksheet(&xml, &mut sheet, &context)
            .map_err(|detail| SheetError::bad_xml(&part, detail))?;
        read_sheet_relations(state, &part, &links, &mut sheet)?;
        let ids = SheetIds {
            hyperlinks: links.hyperlink_ids.clone(),
            ..SheetIds::default()
        };
        remember_generated(&mut sheet, &ids);
        workbook.sheets.push(sheet);
    }
    Ok(())
}

/// excel keeps print areas and repeated titles as hidden defined names;
/// the model keeps them with the sheet's print settings
fn move_print_names_to_sheets(workbook: &mut Workbook) {
    let mut kept = Vec::new();
    for name in std::mem::take(&mut workbook.defined_names) {
        let target = name.sheet.filter(|index| *index < workbook.sheets.len());
        let is_area = name.name.eq_ignore_ascii_case("_xlnm.Print_Area");
        let is_titles = name.name.eq_ignore_ascii_case("_xlnm.Print_Titles");
        match (target, is_area, is_titles) {
            (Some(index), true, _) => {
                workbook.sheets[index].print.area = print_ranges(&name.formula)
            }
            (Some(index), _, true) => {
                apply_print_titles(&mut workbook.sheets[index], &name.formula)
            }
            _ => kept.push(name),
        }
    }
    workbook.defined_names = kept;
}

/// "'My sheet'!$A$1:$C$9,'My sheet'!$E$1:$F$2" -> the ranges
fn print_ranges(formula: &str) -> Vec<crate::address::Range> {
    let Ok(expr) = crate::formula::parse(&format!("({formula})")) else {
        return Vec::new();
    };
    let mut ranges = Vec::new();
    expr.walk(&mut |node| {
        if let crate::formula::Expr::Ref { reference } = node {
            ranges.push(reference.area.to_range());
        }
    });
    ranges
}

fn apply_print_titles(sheet: &mut Sheet, formula: &str) {
    for range in print_ranges(formula) {
        if range.is_whole_rows() {
            sheet.print.title_rows = Some((range.start.row, range.end.row));
        } else if range.is_whole_columns() {
            sheet.print.title_cols = Some((range.start.col, range.end.col));
        }
    }
}

fn read_core_properties(
    state: &mut ReadState,
    rels: &[Relationship],
    workbook: &mut Workbook,
) -> Result<(), SheetError> {
    let Some(part) = target_of(rels, CORE_PROPERTIES_REL, "") else {
        return Ok(());
    };
    let Some(xml) = state.text(&part)? else {
        return Ok(());
    };
    let mut reader = Reader::new(&xml);
    let mut metadata = Metadata::default();
    while let Ok(Some(event)) = reader.next() {
        let Event::Start(tag) = event else { continue };
        let slot = match tag.local() {
            "title" => &mut metadata.title,
            "subject" => &mut metadata.subject,
            "creator" => &mut metadata.creator,
            "keywords" => &mut metadata.keywords,
            "description" => &mut metadata.description,
            "lastModifiedBy" => &mut metadata.last_modified_by,
            "created" => &mut metadata.created,
            "modified" => &mut metadata.modified,
            _ => continue,
        };
        *slot = reader
            .element_text(&tag)
            .ok()
            .filter(|text| !text.is_empty());
    }
    let made = super::write_workbook_part::core_properties_xml(&metadata);
    workbook.metadata = metadata;
    workbook.carry.core_properties = Some((xml, fingerprint(&made)));
    Ok(())
}

/// every part not rebuilt on save, kept byte for byte. the calculation
/// chain is dropped: it lists formula cells, and a stale one makes excel
/// repair the file
fn carry_remaining_parts(state: &mut ReadState, types: &[ContentType], workbook: &mut Workbook) {
    for part in &state.package.parts {
        let lower = part.name.to_ascii_lowercase();
        if state.used.contains(&lower) || lower.ends_with("calcchain.xml") {
            continue;
        }
        workbook.carry.parts.push(CarriedPart {
            name: part.name.clone(),
            bytes: part.bytes.clone(),
        });
        if let Some(entry) = types
            .iter()
            .find(|t| !t.is_default && t.key.eq_ignore_ascii_case(&part.name))
        {
            workbook.carry.content_types.push((
                entry.key.clone(),
                entry.content_type.clone(),
                false,
            ));
        }
    }
    for entry in types.iter().filter(|t| t.is_default) {
        workbook
            .carry
            .content_types
            .push((entry.key.clone(), entry.content_type.clone(), true));
    }
}

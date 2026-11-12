//! writing a whole xlsx package: the workbook part, styles, shared
//! strings, every worksheet with its notes and tables, the relationship
//! and content type lists, document properties, and every carried part.

use std::collections::HashSet;

use crate::error::SheetError;
use crate::model::carry::{fingerprint, Relationship};
use crate::model::*;
use crate::package::{relative_target, write_zip, Part};

use super::read_sheet_parts::comments_fingerprint;
use super::relationships::*;
use super::write_cells::StringTable;
use super::write_extras::{comments_xml, notes_vml, table_xml};
use super::write_sheet::{write_worksheet, SheetIds};
use super::write_workbook_part::{core_properties_xml, workbook_part_xml};

const MAIN_PART: &str = "xl/workbook.xml";

/// the package being assembled
struct Output {
    parts: Vec<Part>,
    names: HashSet<String>,
    overrides: Vec<ContentType>,
}

impl Output {
    fn add(&mut self, name: &str, bytes: Vec<u8>, content_type: Option<&str>) {
        self.names.insert(name.to_ascii_lowercase());
        if let Some(content_type) = content_type {
            self.overrides.push(ContentType {
                key: name.to_string(),
                content_type: content_type.to_string(),
                is_default: false,
            });
        }
        self.parts.push(Part {
            name: name.to_string(),
            bytes,
        });
    }

    fn taken(&self, name: &str) -> bool {
        self.names.contains(&name.to_ascii_lowercase())
    }

    /// "xl/tables/table3.xml" for the first number not in use
    fn fresh_name(&mut self, stem: &str, extension: &str) -> String {
        let name = (1..)
            .map(|n| format!("{stem}{n}.{extension}"))
            .find(|name| !self.taken(name))
            .unwrap_or_default();
        self.names.insert(name.to_ascii_lowercase());
        name
    }
}

pub fn write_xlsx(workbook: &Workbook) -> Result<Vec<u8>, SheetError> {
    let mut output = Output {
        parts: Vec::new(),
        names: HashSet::new(),
        overrides: Vec::new(),
    };
    reserve_carried_names(workbook, &mut output);
    let main = main_part_name(workbook);
    let mut strings = StringTable::default();
    let mut workbook_rels: Vec<Relationship> = workbook.carry.workbook_relationships.clone();
    let mut sheet_ids = Vec::new();
    for (index, sheet) in workbook.sheets.iter().enumerate() {
        let part = sheet_part_name(sheet, &mut output);
        write_sheet_parts(workbook, index, &part, &mut strings, &mut output);
        let id = fresh_id(&workbook_rels, "S");
        workbook_rels.push(Relationship {
            id: id.clone(),
            kind: WORKSHEET_REL.into(),
            target: relative_target(&main, &part),
            mode: None,
        });
        sheet_ids.push(id);
    }
    add_styles_and_strings(workbook, &main, &strings, &mut workbook_rels, &mut output);
    add_dynamic_array_metadata(workbook, &main, &mut workbook_rels, &mut output);
    let workbook_xml = workbook_part_xml(workbook, &sheet_ids);
    let main_type = workbook
        .carry
        .main_content_type
        .clone()
        .unwrap_or_else(|| WORKBOOK_TYPE.into());
    output.add(&main, workbook_xml.into_bytes(), Some(&main_type));
    output.add(
        &rels_part_for(&main),
        write_relationships(&workbook_rels).into_bytes(),
        None,
    );
    add_package_parts(workbook, &main, &mut output);
    add_carried_parts(workbook, &mut output);
    let types = content_types(workbook, &output);
    let mut parts = vec![Part {
        name: "[Content_Types].xml".into(),
        bytes: write_content_types(&types).into_bytes(),
    }];
    parts.append(&mut output.parts);
    write_zip(&parts, &[])
}

fn main_part_name(workbook: &Workbook) -> String {
    workbook
        .carry
        .package_relationships
        .iter()
        .find(|rel| rel.kind == OFFICE_DOCUMENT_REL)
        .map(|rel| crate::package::resolve_target("", &rel.target))
        .unwrap_or_else(|| MAIN_PART.to_string())
}

/// carried parts keep their names, so new parts must not take them
fn reserve_carried_names(workbook: &Workbook, output: &mut Output) {
    for part in &workbook.carry.parts {
        output.names.insert(part.name.to_ascii_lowercase());
    }
}

fn sheet_part_name(sheet: &Sheet, output: &mut Output) -> String {
    match &sheet.carry.part {
        Some(part) if !output.taken(part) => {
            output.names.insert(part.to_ascii_lowercase());
            part.clone()
        }
        _ => output.fresh_name("xl/worksheets/sheet", "xml"),
    }
}

fn write_sheet_parts(
    workbook: &Workbook,
    index: usize,
    part: &str,
    strings: &mut StringTable,
    output: &mut Output,
) {
    let sheet = &workbook.sheets[index];
    let mut rels = sheet.carry.relationships.clone();
    let mut ids = SheetIds {
        hyperlinks: hyperlink_ids(sheet, &mut rels),
        ..SheetIds::default()
    };
    if !sheet.comments.is_empty() {
        ids.legacy_drawing = Some(add_comments(sheet, part, &mut rels, output));
    }
    for table in &sheet.tables {
        ids.tables
            .push(add_table(sheet, table, part, &mut rels, output));
    }
    let xml = write_worksheet(sheet, &ids, strings);
    output.add(part, xml.into_bytes(), Some(WORKSHEET_TYPE));
    if !rels.is_empty() {
        output.add(
            &rels_part_for(part),
            write_relationships(&rels).into_bytes(),
            None,
        );
    }
}

/// each external hyperlink's relationship: an existing one to the same
/// address is reused, else one is added
fn hyperlink_ids(sheet: &Sheet, rels: &mut Vec<Relationship>) -> Vec<Option<String>> {
    let mut ids = Vec::new();
    for link in &sheet.hyperlinks {
        let Some(target) = &link.target else {
            ids.push(None);
            continue;
        };
        let existing = rels
            .iter()
            .find(|rel| rel.kind == HYPERLINK_REL && rel.target == *target);
        let id = match existing {
            Some(rel) => rel.id.clone(),
            None => {
                let id = fresh_id(rels, "H");
                rels.push(Relationship {
                    id: id.clone(),
                    kind: HYPERLINK_REL.into(),
                    target: target.clone(),
                    mode: Some("External".into()),
                });
                id
            }
        };
        ids.push(Some(id));
    }
    ids
}

/// writes the notes and their vml drawing; returns the vml's relationship id
fn add_comments(
    sheet: &Sheet,
    part: &str,
    rels: &mut Vec<Relationship>,
    output: &mut Output,
) -> String {
    let carried = sheet
        .carry
        .comments
        .as_ref()
        .filter(|carried| carried.fingerprint == comments_fingerprint(&sheet.comments));
    let (comments_part, comments, vml_part, vml) = match carried {
        Some(carried) if carried.vml_part.is_some() && !output.taken(&carried.part) => (
            carried.part.clone(),
            carried.xml.clone(),
            carried.vml_part.clone().unwrap_or_default(),
            carried
                .vml_xml
                .clone()
                .unwrap_or_else(|| notes_vml(&sheet.comments)),
        ),
        _ => (
            output.fresh_name("xl/comments", "xml"),
            comments_xml(&sheet.comments),
            output.fresh_name("xl/drawings/vmlDrawing", "vml"),
            notes_vml(&sheet.comments),
        ),
    };
    output.add(&comments_part, comments.into_bytes(), Some(COMMENTS_TYPE));
    output.add(&vml_part, vml.into_bytes(), None);
    let comments_id = fresh_id(rels, "C");
    rels.push(Relationship {
        id: comments_id,
        kind: COMMENTS_REL.into(),
        target: relative_target(part, &comments_part),
        mode: None,
    });
    let vml_id = fresh_id(rels, "V");
    rels.push(Relationship {
        id: vml_id.clone(),
        kind: VML_REL.into(),
        target: relative_target(part, &vml_part),
        mode: None,
    });
    vml_id
}

fn add_table(
    sheet: &Sheet,
    table: &Table,
    part: &str,
    rels: &mut Vec<Relationship>,
    output: &mut Output,
) -> String {
    let made = table_xml(table);
    let carried = sheet
        .carry
        .tables
        .iter()
        .find(|(id, name, _, _)| *id == table.id && !output.taken(name));
    let (table_part, xml) = match carried {
        Some((_, name, raw, generated)) if *generated == fingerprint(&made) => {
            (name.clone(), raw.clone())
        }
        Some((_, name, _, _)) => (name.clone(), made),
        None => (output.fresh_name("xl/tables/table", "xml"), made),
    };
    output.add(&table_part, xml.into_bytes(), Some(TABLE_TYPE));
    let id = fresh_id(rels, "T");
    rels.push(Relationship {
        id: id.clone(),
        kind: TABLE_REL.into(),
        target: relative_target(part, &table_part),
        mode: None,
    });
    id
}

fn add_styles_and_strings(
    workbook: &Workbook,
    main: &str,
    strings: &StringTable,
    rels: &mut Vec<Relationship>,
    output: &mut Output,
) {
    let carry = &workbook.carry;
    let styles = super::styles::write_styles(
        &workbook.styles,
        &carry.styles_sections,
        &carry.styles_root_attributes,
    );
    output.add("xl/styles.xml", styles.into_bytes(), Some(STYLES_TYPE));
    rels.push(Relationship {
        id: fresh_id(rels, "St"),
        kind: STYLES_REL.into(),
        target: relative_target(main, "xl/styles.xml"),
        mode: None,
    });
    output.add(
        "xl/sharedStrings.xml",
        strings.to_xml().into_bytes(),
        Some(SHARED_STRINGS_TYPE),
    );
    rels.push(Relationship {
        id: fresh_id(rels, "Ss"),
        kind: SHARED_STRINGS_REL.into(),
        target: relative_target(main, "xl/sharedStrings.xml"),
        mode: None,
    });
}

const METADATA_REL: &str =
    "http://schemas.openxmlformats.org/officeDocument/2006/relationships/sheetMetadata";
const METADATA_TYPE: &str =
    "application/vnd.openxmlformats-officedocument.spreadsheetml.sheetMetadata+xml";

/// spilling formulas need excel's dynamic array metadata; a file that
/// already carries a metadata part keeps its own (excel writes the
/// dynamic array type first in it, so cm="1" means the same)
fn add_dynamic_array_metadata(
    workbook: &Workbook,
    main: &str,
    rels: &mut Vec<Relationship>,
    output: &mut Output,
) {
    let spills = workbook.sheets.iter().any(|sheet| {
        sheet
            .cells
            .iter()
            .any(|(_, _, cell)| super::write_cells::is_spilling(cell))
    });
    let carried = rels.iter().any(|rel| rel.kind == METADATA_REL);
    if !spills || carried {
        return;
    }
    let part = output.fresh_name("xl/metadata", "xml");
    output.add(
        &part,
        super::write_cells::DYNAMIC_ARRAY_METADATA
            .as_bytes()
            .to_vec(),
        Some(METADATA_TYPE),
    );
    rels.push(Relationship {
        id: fresh_id(rels, "Md"),
        kind: METADATA_REL.into(),
        target: relative_target(main, &part),
        mode: None,
    });
}

/// the package relationships and document properties
fn add_package_parts(workbook: &Workbook, main: &str, output: &mut Output) {
    let mut rels = workbook.carry.package_relationships.clone();
    if !rels.iter().any(|rel| rel.kind == OFFICE_DOCUMENT_REL) {
        rels.push(Relationship {
            id: fresh_id(&rels, "M"),
            kind: OFFICE_DOCUMENT_REL.into(),
            target: main.to_string(),
            mode: None,
        });
    }
    let core_part = rels
        .iter()
        .find(|rel| rel.kind == CORE_PROPERTIES_REL)
        .map(|rel| crate::package::resolve_target("", &rel.target));
    let made = core_properties_xml(&workbook.metadata);
    let core = match (&workbook.carry.core_properties, &core_part) {
        (Some((raw, generated)), Some(_)) if *generated == fingerprint(&made) => Some(raw.clone()),
        (_, Some(_)) => Some(made),
        (None, None) if workbook.metadata != Metadata::default() => Some(made),
        _ => None,
    };
    if let Some(core) = core {
        let part = core_part.unwrap_or_else(|| {
            rels.push(Relationship {
                id: fresh_id(&rels, "P"),
                kind: CORE_PROPERTIES_REL.into(),
                target: "docProps/core.xml".into(),
                mode: None,
            });
            "docProps/core.xml".to_string()
        });
        output.add(&part, core.into_bytes(), Some(CORE_PROPERTIES_TYPE));
    }
    output.add("_rels/.rels", write_relationships(&rels).into_bytes(), None);
}

fn add_carried_parts(workbook: &Workbook, output: &mut Output) {
    for part in &workbook.carry.parts {
        output.parts.push(Part {
            name: part.name.clone(),
            bytes: part.bytes.clone(),
        });
    }
}

fn content_types(workbook: &Workbook, output: &Output) -> Vec<ContentType> {
    let mut types: Vec<ContentType> = workbook
        .carry
        .content_types
        .iter()
        .map(|(key, content_type, is_default)| ContentType {
            key: key.clone(),
            content_type: content_type.clone(),
            is_default: *is_default,
        })
        .collect();
    let defaults = [
        ("rels", RELATIONSHIPS_TYPE),
        ("xml", "application/xml"),
        ("vml", VML_TYPE),
    ];
    for (extension, content_type) in defaults {
        if !types
            .iter()
            .any(|t| t.is_default && t.key.eq_ignore_ascii_case(extension))
        {
            types.push(ContentType {
                key: extension.into(),
                content_type: content_type.into(),
                is_default: true,
            });
        }
    }
    // an override from the file for a part rewritten here is replaced
    types.retain(|t| {
        t.is_default
            || !output
                .overrides
                .iter()
                .any(|o| o.key.eq_ignore_ascii_case(&t.key))
    });
    types.extend(output.overrides.iter().cloned());
    types
}

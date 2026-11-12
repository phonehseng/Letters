//! one worksheet part: every top-level element is either read into the
//! model or kept as its original xml under its name, so the writer can
//! put it back in schema order.

use crate::model::carry::Section;
use crate::model::Sheet;
use crate::xml::{Event, Reader};

use super::read_cells::{read_sheet_data, CellContext};
use super::read_sections::*;

/// the relationship ids a worksheet uses, resolved once its rels are read
#[derive(Default)]
pub struct SheetLinks {
    pub hyperlink_ids: Vec<Option<String>>,
    pub table_ids: Vec<String>,
}

/// worksheet elements the model reads (and the writer makes) rather
/// than keeps; `dimension` is always remade from the cells
pub const MODELLED: [&str; 18] = [
    "sheetPr",
    "dimension",
    "sheetViews",
    "sheetFormatPr",
    "cols",
    "autoFilter",
    "mergeCells",
    "conditionalFormatting",
    "dataValidations",
    "hyperlinks",
    "printOptions",
    "pageMargins",
    "pageSetup",
    "headerFooter",
    "rowBreaks",
    "colBreaks",
    "legacyDrawing",
    "tableParts",
];

pub fn read_worksheet(
    xml: &str,
    sheet: &mut Sheet,
    context: &CellContext,
) -> Result<SheetLinks, String> {
    let mut reader = Reader::new(xml);
    let root = reader.root()?;
    sheet.carry.root_attributes = root.owned_attributes();
    let mut links = SheetLinks::default();
    loop {
        let tag = match reader.next()? {
            Some(Event::Start(tag)) => tag,
            Some(Event::End { .. }) | None => break,
            Some(Event::Text(_)) => continue,
        };
        if tag.local() == "sheetData" {
            if tag.self_closing {
                reader.next()?;
            } else {
                read_sheet_data(&mut reader, sheet, context)?;
            }
            continue;
        }
        let name = tag.local().to_string();
        let raw = reader.skip_element(&tag)?;
        read_element(&name, raw, sheet, context, &mut links);
        remember(sheet, &name, raw);
    }
    Ok(links)
}

/// keeps an element's original xml; repeated elements (one
/// conditionalFormatting per range) join one section
fn remember(sheet: &mut Sheet, name: &str, raw: &str) {
    if let Some(section) = sheet.carry.sections.iter_mut().find(|s| s.name == name) {
        section.xml.push_str(raw);
        return;
    }
    sheet.carry.sections.push(Section {
        name: name.to_string(),
        xml: raw.to_string(),
        generated: None,
    });
}

fn read_element(
    name: &str,
    raw: &str,
    sheet: &mut Sheet,
    context: &CellContext,
    links: &mut SheetLinks,
) {
    match name {
        "sheetPr" => read_sheet_properties(raw, sheet),
        "sheetViews" => read_sheet_views(raw, sheet),
        "sheetFormatPr" => read_format_properties(raw, sheet),
        "cols" => read_columns(raw, sheet, context.style_count),
        "sheetProtection" => sheet.protected = !raw.contains("sheet=\"0\""),
        "autoFilter" => sheet.auto_filter = read_auto_filter(raw),
        "mergeCells" => read_merges(raw, sheet),
        "conditionalFormatting" => read_conditional_format(raw, sheet),
        "dataValidations" => read_validations(raw, sheet),
        "hyperlinks" => links.hyperlink_ids = read_hyperlinks(raw, sheet),
        "printOptions" => read_print_options(raw, sheet),
        "pageMargins" => read_margins(raw, sheet),
        "pageSetup" => read_page_setup(raw, sheet),
        "headerFooter" => read_header_footer(raw, sheet),
        "rowBreaks" => sheet.print.row_breaks = read_breaks(raw),
        "colBreaks" => sheet.print.col_breaks = read_breaks(raw),
        "tableParts" => links.table_ids = relationship_ids(raw),
        _ => {}
    }
}

/// the r:id of each child, as tableParts lists its tables
fn relationship_ids(raw: &str) -> Vec<String> {
    let Some(fragment) = crate::xml::Fragment::parse(raw) else {
        return Vec::new();
    };
    fragment
        .children
        .iter()
        .filter_map(|(_, child)| {
            crate::xml::Fragment::parse(child)?
                .attribute("r:id")
                .map(str::to_string)
        })
        .collect()
}

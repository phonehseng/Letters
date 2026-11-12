//! the styles part (`xl/styles.xml`): read into the deduplicated style
//! table, and written back with the file's own fonts, fills, borders and
//! number format ids kept at their original positions, because named
//! styles and differential formats the model does not rebuild refer to
//! them by position.

use crate::model::carry::Section;
use crate::model::style::{Alignment, Border, CellStyle, Fill, Font, Protection, StyleTable};
use crate::numfmt::builtin::{builtin_code, builtin_id, FIRST_CUSTOM_ID};
use crate::xml::{Event, Fragment, Reader, Tag, XmlWriter};

use super::style_parts::*;

const MAIN_NS: &str = "http://schemas.openxmlformats.org/spreadsheetml/2006/main";

/// styles.xml children written back as they were, in schema order
const KEPT_SECTIONS: [&str; 5] = [
    "cellStyleXfs",
    "cellStyles",
    "tableStyles",
    "colors",
    "extLst",
];

/// one `<xf>` before its pieces are looked up
#[derive(Default)]
struct RawXf {
    number_format: u32,
    font: usize,
    fill: usize,
    border: usize,
    named_style: u32,
    alignment: Alignment,
    protection: Protection,
}

pub struct ReadStyles {
    pub table: StyleTable,
    pub sections: Vec<Section>,
    pub root_attributes: Vec<(String, String)>,
}

pub fn read_styles(xml: &str) -> Result<ReadStyles, String> {
    let mut reader = Reader::new(xml);
    let root = reader.root()?;
    let mut table = StyleTable::default();
    let mut sections = Vec::new();
    let mut formats: Vec<(u32, String)> = Vec::new();
    let mut xfs: Vec<RawXf> = Vec::new();
    while let Some(event) = reader.next()? {
        let Event::Start(tag) = event else { continue };
        match tag.local() {
            "numFmts" => formats = read_number_formats(&mut reader)?,
            "fonts" => table.original.fonts = read_list(&mut reader, &tag, parse_font)?,
            "fills" => table.original.fills = read_list(&mut reader, &tag, parse_fill)?,
            "borders" => table.original.borders = read_list(&mut reader, &tag, parse_border)?,
            "cellXfs" => xfs = read_xfs(&mut reader, &tag)?,
            "dxfs" => table.differential = read_differential(&mut reader, &tag)?,
            name => {
                let raw = reader.skip_element(&tag)?;
                sections.push(Section {
                    name: name.to_string(),
                    xml: raw.to_string(),
                    generated: None,
                });
            }
        }
    }
    table.original.number_formats = formats.clone();
    table.styles = xfs
        .iter()
        .map(|xf| resolve_xf(xf, &table, &formats))
        .collect();
    if table.styles.is_empty() {
        table.styles.push(CellStyle::default());
    }
    table.original.default_font = table.original.fonts.first().map(|(font, _)| font.clone());
    Ok(ReadStyles {
        table,
        sections,
        root_attributes: root.owned_attributes(),
    })
}

fn read_number_formats(reader: &mut Reader) -> Result<Vec<(u32, String)>, String> {
    let mut formats = Vec::new();
    loop {
        match reader.next()? {
            Some(Event::Start(tag)) if tag.local() == "numFmt" => {
                let id = tag.attr("numFmtId").and_then(|v| v.parse().ok());
                let code = tag.attr("formatCode").map(|v| v.into_owned());
                if let (Some(id), Some(code)) = (id, code) {
                    formats.push((id, code));
                }
                reader.element_end(&tag)?;
            }
            Some(Event::Start(tag)) => {
                reader.element_end(&tag)?;
            }
            Some(Event::End { .. }) | None => return Ok(formats),
            Some(Event::Text(_)) => {}
        }
    }
}

/// the children of `<fonts>`, `<fills>`, `<borders>` or `<dxfs>`, each
/// parsed and kept as original xml
fn read_list<T>(
    reader: &mut Reader,
    parent: &Tag,
    parse: impl Fn(&str) -> T,
) -> Result<Vec<(T, String)>, String> {
    let mut items = Vec::new();
    if parent.self_closing {
        reader.next()?;
        return Ok(items);
    }
    loop {
        match reader.next()? {
            Some(Event::Start(tag)) => {
                let raw = reader.skip_element(&tag)?;
                items.push((parse(raw), raw.to_string()));
            }
            Some(Event::End { .. }) | None => return Ok(items),
            Some(Event::Text(_)) => {}
        }
    }
}

/// differential formats are kept as their xml
fn read_differential(reader: &mut Reader, parent: &Tag) -> Result<Vec<String>, String> {
    let items = read_list(reader, parent, |_| ())?;
    Ok(items.into_iter().map(|(_, raw)| raw).collect())
}

fn read_xfs(reader: &mut Reader, parent: &Tag) -> Result<Vec<RawXf>, String> {
    let raws = read_list(reader, parent, |raw| raw.to_string())?;
    Ok(raws.into_iter().map(|(raw, _)| parse_xf(&raw)).collect())
}

fn parse_xf(raw: &str) -> RawXf {
    let Some(fragment) = Fragment::parse(raw) else {
        return RawXf::default();
    };
    let number = |name: &str| {
        fragment
            .attribute(name)
            .and_then(|v| v.parse::<u32>().ok())
            .unwrap_or(0)
    };
    let mut xf = RawXf {
        number_format: number("numFmtId"),
        font: number("fontId") as usize,
        fill: number("fillId") as usize,
        border: number("borderId") as usize,
        named_style: number("xfId"),
        ..RawXf::default()
    };
    for (name, child) in &fragment.children {
        let Some(part) = Fragment::parse(child) else {
            continue;
        };
        match name.as_str() {
            "alignment" => xf.alignment = parse_alignment(&part),
            "protection" => {
                xf.protection = Protection {
                    locked: part.attribute("locked") != Some("0"),
                    hidden: part.attribute("hidden") == Some("1"),
                }
            }
            _ => {}
        }
    }
    xf
}

fn parse_alignment(part: &Fragment) -> Alignment {
    let number = |name: &str| part.attribute(name).and_then(|v| v.parse().ok());
    Alignment {
        horizontal: part.attribute("horizontal").map(str::to_string),
        vertical: part.attribute("vertical").map(str::to_string),
        wrap_text: part.attribute("wrapText") == Some("1"),
        shrink_to_fit: part.attribute("shrinkToFit") == Some("1"),
        indent: number("indent"),
        rotation: number("textRotation"),
        reading_order: number("readingOrder"),
    }
}

fn resolve_xf(xf: &RawXf, table: &StyleTable, formats: &[(u32, String)]) -> CellStyle {
    let code = formats
        .iter()
        .find(|(id, _)| *id == xf.number_format)
        .map(|(_, code)| code.clone())
        .or_else(|| builtin_code(xf.number_format).map(str::to_string))
        .unwrap_or_else(|| "General".into());
    let original = &table.original;
    CellStyle {
        number_format: code,
        font: original
            .fonts
            .get(xf.font)
            .map(|(f, _)| f.clone())
            .unwrap_or_default(),
        fill: original
            .fills
            .get(xf.fill)
            .map(|(f, _)| f.clone())
            .unwrap_or_default(),
        border: original
            .borders
            .get(xf.border)
            .map(|(b, _)| b.clone())
            .unwrap_or_default(),
        alignment: xf.alignment.clone(),
        protection: xf.protection.clone(),
        named_style: xf.named_style,
    }
}

/// the position of `item` in the file's list, or after it among the new
/// ones (added on first use)
fn position_of<T: PartialEq + Clone>(
    item: &T,
    original: &[(T, String)],
    added: &mut Vec<T>,
) -> usize {
    if let Some(index) = original.iter().position(|(known, _)| known == item) {
        return index;
    }
    if let Some(index) = added.iter().position(|known| known == item) {
        return original.len() + index;
    }
    added.push(item.clone());
    original.len() + added.len() - 1
}

/// the pieces a written styles part is built from
struct Lists {
    fonts: Vec<Font>,
    fills: Vec<Fill>,
    borders: Vec<Border>,
    formats: Vec<(u32, String)>,
}

pub fn write_styles(
    table: &StyleTable,
    sections: &[Section],
    root_attributes: &[(String, String)],
) -> String {
    let original = &table.original;
    let mut lists = Lists {
        fonts: Vec::new(),
        fills: Vec::new(),
        borders: Vec::new(),
        formats: original.number_formats.clone(),
    };
    if original.fills.is_empty() {
        // excel insists on these two fills first, whatever the styles use
        lists.fills.push(Fill::default());
        lists.fills.push(Fill {
            pattern: Some("gray125".into()),
            ..Fill::default()
        });
    }
    let xfs: Vec<String> = table
        .styles
        .iter()
        .map(|style| write_xf(style, table, &mut lists))
        .collect();
    let mut writer = XmlWriter::with_declaration();
    let mut attributes = root_attributes.to_vec();
    if attributes.is_empty() {
        attributes.push(("xmlns".into(), MAIN_NS.into()));
    }
    writer.open_owned("styleSheet", &attributes);
    write_number_formats(&mut writer, &lists.formats);
    write_counted(
        &mut writer,
        "fonts",
        original
            .fonts
            .iter()
            .map(|(_, raw)| raw.clone())
            .chain(lists.fonts.iter().map(|f| write_font(f, "font"))),
    );
    write_counted(
        &mut writer,
        "fills",
        original
            .fills
            .iter()
            .map(|(_, raw)| raw.clone())
            .chain(lists.fills.iter().map(write_fill)),
    );
    write_counted(
        &mut writer,
        "borders",
        original
            .borders
            .iter()
            .map(|(_, raw)| raw.clone())
            .chain(lists.borders.iter().map(write_border)),
    );
    write_section(&mut writer, sections, "cellStyleXfs", DEFAULT_STYLE_XFS);
    write_counted(&mut writer, "cellXfs", xfs.into_iter());
    write_section(&mut writer, sections, "cellStyles", DEFAULT_CELL_STYLES);
    write_counted(&mut writer, "dxfs", table.differential.iter().cloned());
    for name in &KEPT_SECTIONS[2..] {
        write_section(&mut writer, sections, name, "");
    }
    writer.close("styleSheet");
    writer.finish()
}

/// what a new workbook needs when the file had no named styles
const DEFAULT_STYLE_XFS: &str = r#"<cellStyleXfs count="1"><xf numFmtId="0" fontId="0" fillId="0" borderId="0"/></cellStyleXfs>"#;
const DEFAULT_CELL_STYLES: &str =
    r#"<cellStyles count="1"><cellStyle name="Normal" xfId="0" builtinId="0"/></cellStyles>"#;

fn write_section(writer: &mut XmlWriter, sections: &[Section], name: &str, fallback: &str) {
    match sections.iter().find(|section| section.name == name) {
        Some(section) => writer.raw(&section.xml),
        None => writer.raw(fallback),
    }
}

fn write_counted(writer: &mut XmlWriter, name: &str, items: impl Iterator<Item = String>) {
    let items: Vec<String> = items.collect();
    if items.is_empty() && name == "dxfs" {
        return;
    }
    writer.open(name, &[("count", &items.len().to_string())]);
    for item in items {
        writer.raw(&item);
    }
    writer.close(name);
}

fn write_number_formats(writer: &mut XmlWriter, formats: &[(u32, String)]) {
    if formats.is_empty() {
        return;
    }
    writer.open("numFmts", &[("count", &formats.len().to_string())]);
    for (id, code) in formats {
        writer.empty(
            "numFmt",
            &[("numFmtId", &id.to_string()), ("formatCode", code)],
        );
    }
    writer.close("numFmts");
}

fn number_format_id(code: &str, formats: &mut Vec<(u32, String)>) -> u32 {
    if let Some((id, _)) = formats.iter().find(|(_, known)| known == code) {
        return *id;
    }
    if let Some(id) = builtin_id(code) {
        return id;
    }
    let next = formats
        .iter()
        .map(|(id, _)| id + 1)
        .max()
        .unwrap_or(FIRST_CUSTOM_ID)
        .max(FIRST_CUSTOM_ID);
    formats.push((next, code.to_string()));
    next
}

fn write_xf(style: &CellStyle, table: &StyleTable, lists: &mut Lists) -> String {
    let original = &table.original;
    let format = number_format_id(&style.number_format, &mut lists.formats);
    let font = position_of(&style.font, &original.fonts, &mut lists.fonts);
    let fill = if original.fills.is_empty() {
        lists
            .fills
            .iter()
            .position(|f| *f == style.fill)
            .unwrap_or_else(|| {
                lists.fills.push(style.fill.clone());
                lists.fills.len() - 1
            })
    } else {
        position_of(&style.fill, &original.fills, &mut lists.fills)
    };
    let border = position_of(&style.border, &original.borders, &mut lists.borders);
    let mut attributes = vec![
        ("numFmtId".to_string(), format.to_string()),
        ("fontId".to_string(), font.to_string()),
        ("fillId".to_string(), fill.to_string()),
        ("borderId".to_string(), border.to_string()),
        ("xfId".to_string(), style.named_style.to_string()),
    ];
    let flags = [
        ("applyNumberFormat", format != 0),
        ("applyFont", font != 0),
        ("applyFill", fill != 0),
        ("applyBorder", border != 0),
        ("applyAlignment", style.alignment != Alignment::default()),
        ("applyProtection", style.protection != Protection::default()),
    ];
    for (name, on) in flags {
        if on {
            attributes.push((name.to_string(), "1".to_string()));
        }
    }
    let mut writer = XmlWriter::new();
    let alignment = alignment_attributes(&style.alignment);
    let protection = style.protection != Protection::default();
    if alignment.is_empty() && !protection {
        writer.empty_owned("xf", &attributes);
        return writer.finish();
    }
    writer.open_owned("xf", &attributes);
    if !alignment.is_empty() {
        writer.empty_owned("alignment", &alignment);
    }
    if protection {
        let locked = if style.protection.locked { "1" } else { "0" };
        let hidden = if style.protection.hidden { "1" } else { "0" };
        writer.empty("protection", &[("locked", locked), ("hidden", hidden)]);
    }
    writer.close("xf");
    writer.finish()
}

fn alignment_attributes(alignment: &Alignment) -> Vec<(String, String)> {
    let mut attributes = Vec::new();
    let mut push = |name: &str, value: Option<String>| {
        if let Some(value) = value {
            attributes.push((name.to_string(), value));
        }
    };
    push("horizontal", alignment.horizontal.clone());
    push("vertical", alignment.vertical.clone());
    push("textRotation", alignment.rotation.map(|v| v.to_string()));
    push("wrapText", alignment.wrap_text.then(|| "1".to_string()));
    push("indent", alignment.indent.map(|v| v.to_string()));
    push(
        "shrinkToFit",
        alignment.shrink_to_fit.then(|| "1".to_string()),
    );
    push(
        "readingOrder",
        alignment.reading_order.map(|v| v.to_string()),
    );
    attributes
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"<styleSheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><numFmts count="1"><numFmt numFmtId="164" formatCode="0.000"/></numFmts><fonts count="2"><font><sz val="11"/><name val="Calibri"/></font><font><b/><sz val="11"/><name val="Calibri"/></font></fonts><fills count="2"><fill><patternFill patternType="none"/></fill><fill><patternFill patternType="gray125"/></fill></fills><borders count="1"><border><left/><right/><top/><bottom/><diagonal/></border></borders><cellStyleXfs count="1"><xf numFmtId="0" fontId="0" fillId="0" borderId="0"/></cellStyleXfs><cellXfs count="3"><xf numFmtId="0" fontId="0" fillId="0" borderId="0" xfId="0"/><xf numFmtId="164" fontId="1" fillId="0" borderId="0" xfId="0" applyFont="1"><alignment horizontal="center"/></xf><xf numFmtId="14" fontId="0" fillId="0" borderId="0" xfId="0"/></cellXfs><cellStyles count="1"><cellStyle name="Normal" xfId="0" builtinId="0"/></cellStyles><dxfs count="1"><dxf><font><color rgb="FF9C0006"/></font></dxf></dxfs></styleSheet>"#;

    #[test]
    fn reads_styles_and_writes_them_back_equal() {
        let read = read_styles(SAMPLE).unwrap();
        assert_eq!(read.table.styles.len(), 3);
        assert_eq!(read.table.styles[1].number_format, "0.000");
        assert!(read.table.styles[1].font.bold);
        assert_eq!(
            read.table.styles[1].alignment.horizontal.as_deref(),
            Some("center")
        );
        assert_eq!(read.table.styles[2].number_format, "m/d/yyyy");
        assert_eq!(read.table.differential.len(), 1);
        let written = write_styles(&read.table, &read.sections, &read.root_attributes);
        let again = read_styles(&written).unwrap();
        assert_eq!(again.table.styles, read.table.styles);
        assert_eq!(again.table.differential, read.table.differential);
    }

    #[test]
    fn new_styles_are_appended_after_the_files_own() {
        let mut read = read_styles(SAMPLE).unwrap();
        read.table.derive(0, |s| {
            s.font.italic = true;
            s.number_format = "0.0%".into();
        });
        let written = write_styles(&read.table, &read.sections, &read.root_attributes);
        assert!(written.contains(r#"<fonts count="3">"#));
        assert!(written.contains(r#"numFmtId="165" formatCode="0.0%""#));
        let again = read_styles(&written).unwrap();
        assert!(again.table.styles[3].font.italic);
    }

    #[test]
    fn a_new_workbook_gets_the_parts_excel_requires() {
        let written = write_styles(&StyleTable::default(), &[], &[]);
        assert!(written.contains("gray125"));
        assert!(written.contains("<name val=\"Calibri\"/>"));
        assert!(written.contains("cellStyleXfs"));
    }
}

//! writing a whole ods package: mimetype first and stored (as the
//! opendocument package rules require), content with the tables, names
//! and validations, the kept or a minimal styles part, settings with the
//! frozen panes, document properties, the manifest and carried parts.

use crate::error::SheetError;
use crate::model::*;
use crate::package::{write_zip, Part};
use crate::xml::{escape_attribute, escape_text};

use super::write_styles::automatic_styles;
use super::write_table::table_xml;

const MIMETYPE: &str = "application/vnd.oasis.opendocument.spreadsheet";

const NAMESPACES: &str = concat!(
    "xmlns:office=\"urn:oasis:names:tc:opendocument:xmlns:office:1.0\" ",
    "xmlns:style=\"urn:oasis:names:tc:opendocument:xmlns:style:1.0\" ",
    "xmlns:text=\"urn:oasis:names:tc:opendocument:xmlns:text:1.0\" ",
    "xmlns:table=\"urn:oasis:names:tc:opendocument:xmlns:table:1.0\" ",
    "xmlns:draw=\"urn:oasis:names:tc:opendocument:xmlns:drawing:1.0\" ",
    "xmlns:fo=\"urn:oasis:names:tc:opendocument:xmlns:xsl-fo-compatible:1.0\" ",
    "xmlns:xlink=\"http://www.w3.org/1999/xlink\" ",
    "xmlns:dc=\"http://purl.org/dc/elements/1.1/\" ",
    "xmlns:meta=\"urn:oasis:names:tc:opendocument:xmlns:meta:1.0\" ",
    "xmlns:number=\"urn:oasis:names:tc:opendocument:xmlns:datastyle:1.0\" ",
    "xmlns:svg=\"urn:oasis:names:tc:opendocument:xmlns:svg-compatible:1.0\" ",
    "xmlns:of=\"urn:oasis:names:tc:opendocument:xmlns:of:1.2\" ",
    "xmlns:config=\"urn:oasis:names:tc:opendocument:xmlns:config:1.0\" ",
    "xmlns:calcext=\"urn:org:documentfoundation:names:experimental:calc:xmlns:calcext:1.0\" ",
    "xmlns:tableooo=\"http://openoffice.org/2009/table\" ",
    "xmlns:loext=\"urn:org:documentfoundation:names:experimental:office:xmlns:loext:1.0\" ",
    "office:version=\"1.3\""
);

/// spreadsheet children kept from the file, by where they go
const BEFORE_TABLES: [&str; 3] = ["calculation-settings", "label-ranges", "tracked-changes"];

pub fn write_ods(book: &Workbook) -> Result<Vec<u8>, SheetError> {
    let mut parts = vec![Part {
        name: "mimetype".into(),
        bytes: MIMETYPE.as_bytes().to_vec(),
    }];
    parts.push(Part {
        name: "content.xml".into(),
        bytes: content_xml(book).into_bytes(),
    });
    parts.push(Part {
        name: "styles.xml".into(),
        bytes: styles_xml(book).into_bytes(),
    });
    parts.push(Part {
        name: "meta.xml".into(),
        bytes: meta_xml(&book.metadata).into_bytes(),
    });
    parts.push(Part {
        name: "settings.xml".into(),
        bytes: settings_xml(book).into_bytes(),
    });
    // parts carried from another format (an xlsx's drawings) mean nothing here
    if book.carry.source == Some(SourceFormat::Ods) {
        for part in &book.carry.parts {
            parts.push(Part {
                name: part.name.clone(),
                bytes: part.bytes.clone(),
            });
        }
    }
    parts.push(Part {
        name: "META-INF/manifest.xml".into(),
        bytes: manifest_xml(book, &parts).into_bytes(),
    });
    write_zip(&parts, &["mimetype"])
}

fn from_ods(book: &Workbook) -> bool {
    book.carry.source == Some(SourceFormat::Ods)
}

fn kept<'a>(book: &'a Workbook, name: &str) -> Option<&'a str> {
    book.carry
        .ods_sections
        .iter()
        .find(|s| s.name == name)
        .map(|s| s.xml.as_str())
}

fn content_xml(book: &Workbook) -> String {
    let (names, styles) = automatic_styles(book);
    let mut out = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<office:document-content {NAMESPACES}>"
    );
    let ods_sections: Vec<&crate::model::carry::Section> = if from_ods(book) {
        book.carry.workbook_sections.iter().collect()
    } else {
        Vec::new()
    };
    for section in ods_sections
        .iter()
        .filter(|s| s.name == "scripts" || s.name == "font-face-decls")
    {
        out.push_str(&section.xml);
    }
    out.push_str(&styles);
    out.push_str("<office:body><office:spreadsheet>");
    for section in ods_sections
        .iter()
        .filter(|s| BEFORE_TABLES.contains(&s.name.as_str()))
    {
        out.push_str(&section.xml);
    }
    out.push_str(&validations_xml(book));
    for index in 0..book.sheets.len() {
        out.push_str(&table_xml(book, index, &names));
    }
    out.push_str(&names_xml(book));
    let after = |s: &&&crate::model::carry::Section| {
        !BEFORE_TABLES.contains(&s.name.as_str())
            && s.name != "scripts"
            && s.name != "font-face-decls"
    };
    for section in ods_sections.iter().filter(after) {
        out.push_str(&section.xml);
    }
    out.push_str("</office:spreadsheet></office:body></office:document-content>");
    out
}

/// "Sheet1!$A$1:$B$2" as "$Sheet1.$A$1:.$B$2" for named ranges
fn open_range(formula: &str) -> Option<String> {
    let expr = crate::formula::parse(formula).ok()?;
    let crate::formula::Expr::Ref { .. } = expr else {
        return None;
    };
    let text = crate::formula::to_open_formula(&expr);
    Some(
        text.trim_start_matches("of:=")
            .trim_start_matches('[')
            .trim_end_matches(']')
            .to_string(),
    )
}

fn names_xml(book: &Workbook) -> String {
    if book.defined_names.is_empty() {
        return String::new();
    }
    let mut out = String::from("<table:named-expressions>");
    for name in &book.defined_names {
        let escaped = escape_attribute(&name.name);
        match open_range(&name.formula) {
            Some(range) => out.push_str(&format!(
                "<table:named-range table:name=\"{escaped}\" table:base-cell-address=\"$Sheet1.$A$1\" table:cell-range-address=\"{}\"/>",
                escape_attribute(&range)
            )),
            None => {
                let expression = crate::formula::parse(&name.formula).map(|e| crate::formula::to_open_formula(&e)).unwrap_or_default();
                out.push_str(&format!(
                    "<table:named-expression table:name=\"{escaped}\" table:base-cell-address=\"$Sheet1.$A$1\" table:expression=\"{}\"/>",
                    escape_attribute(&expression)
                ));
            }
        }
    }
    out.push_str("</table:named-expressions>");
    out
}

/// the opendocument condition for a validation rule
fn condition(rule: &DataValidation) -> Option<String> {
    let open = |text: &Option<String>| {
        text.as_ref().map(|t| {
            crate::formula::parse(t)
                .map(|e| {
                    crate::formula::to_open_formula(&e)
                        .trim_start_matches("of:=")
                        .to_string()
                })
                .unwrap_or_else(|_| t.clone())
        })
    };
    let first = open(&rule.formula1)?;
    if rule.kind == "list" {
        let list = match rule
            .formula1
            .as_deref()
            .and_then(|f| f.strip_prefix('"'))
            .and_then(|f| f.strip_suffix('"'))
        {
            Some(items) => items
                .split(',')
                .map(|i| format!("\"{}\"", i.trim()))
                .collect::<Vec<_>>()
                .join(";"),
            None => first,
        };
        return Some(format!("of:cell-content-is-in-list({list})"));
    }
    let kind = match rule.kind.as_str() {
        "whole" => "cell-content-is-whole-number()",
        "decimal" => "cell-content-is-decimal-number()",
        "date" => "cell-content-is-date()",
        "time" => "cell-content-is-time()",
        "textLength" => "cell-content-text-length()",
        _ => return Some(format!("of:is-true-formula({first})")),
    };
    let operator = rule.operator.as_deref().unwrap_or("between");
    let test = match operator {
        "between" | "notBetween" => {
            let name = if operator == "between" {
                "cell-content-is-between"
            } else {
                "cell-content-is-not-between"
            };
            format!(
                "{name}({first};{})",
                open(&rule.formula2).unwrap_or_default()
            )
        }
        other => {
            let symbol = match other {
                "equal" => "=",
                "notEqual" => "!=",
                "lessThan" => "<",
                "lessThanOrEqual" => "<=",
                "greaterThan" => ">",
                _ => ">=",
            };
            format!("cell-content(){symbol}{first}")
        }
    };
    if rule.kind == "textLength" {
        Some(format!(
            "of:{}",
            test.replacen("cell-content()", "cell-content-text-length()", 1)
        ))
    } else {
        Some(format!("of:{kind} and {test}"))
    }
}

fn validations_xml(book: &Workbook) -> String {
    let mut out = String::new();
    for (sheet_index, sheet) in book.sheets.iter().enumerate() {
        for (index, rule) in sheet.validations.iter().enumerate() {
            let Some(condition) = condition(rule) else {
                continue;
            };
            out.push_str(&format!(
                "<table:content-validation table:name=\"LtrVal{sheet_index}_{index}\" table:condition=\"{}\" table:allow-empty-cell=\"{}\" table:display-list=\"{}\"/>",
                escape_attribute(&condition),
                rule.allow_blank,
                if rule.hide_dropdown { "none" } else { "unsorted" }
            ));
        }
    }
    if out.is_empty() {
        return out;
    }
    format!("<table:content-validations>{out}</table:content-validations>")
}

fn styles_xml(book: &Workbook) -> String {
    if from_ods(book) {
        if let Some(xml) = kept(book, "styles.xml") {
            return xml.to_string();
        }
    }
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<office:document-styles {NAMESPACES}><office:styles><style:default-style style:family=\"table-cell\"><style:text-properties style:font-name=\"Calibri\" fo:font-size=\"11pt\"/></style:default-style><style:style style:name=\"Default\" style:family=\"table-cell\"/></office:styles><office:automatic-styles><style:page-layout style:name=\"Pm1\"/></office:automatic-styles><office:master-styles><style:master-page style:name=\"Default\" style:page-layout-name=\"Pm1\"/></office:master-styles></office:document-styles>"
    )
}

fn settings_xml(book: &Workbook) -> String {
    let unchanged_frozen = book.sheets.iter().all(|s| s.view.frozen.is_none());
    if from_ods(book) && unchanged_frozen {
        if let Some(xml) = kept(book, "settings.xml") {
            return xml.to_string();
        }
    }
    let active = book
        .sheets
        .get(book.active_sheet)
        .map(|s| s.name.clone())
        .unwrap_or_default();
    let mut tables = String::new();
    for sheet in &book.sheets {
        let frozen = sheet.view.frozen.unwrap_or_default();
        let item = |name: &str, kind: &str, value: String| {
            format!("<config:config-item config:name=\"{name}\" config:type=\"{kind}\">{value}</config:config-item>")
        };
        let mode = |n: u32| {
            if n > 0 {
                "2".to_string()
            } else {
                "0".to_string()
            }
        };
        tables.push_str(&format!(
            "<config:config-item-map-entry config:name=\"{}\">{}{}{}{}{}{}{}</config:config-item-map-entry>",
            escape_attribute(&sheet.name),
            item("HorizontalSplitMode", "short", mode(frozen.cols)),
            item("VerticalSplitMode", "short", mode(frozen.rows)),
            item("HorizontalSplitPosition", "int", frozen.cols.to_string()),
            item("VerticalSplitPosition", "int", frozen.rows.to_string()),
            item("PositionRight", "int", frozen.cols.to_string()),
            item("PositionBottom", "int", frozen.rows.to_string()),
            item("ZoomValue", "int", sheet.view.zoom.to_string()),
        ));
    }
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<office:document-settings {NAMESPACES}><office:settings><config:config-item-set config:name=\"ooo:view-settings\"><config:config-item-map-indexed config:name=\"Views\"><config:config-item-map-entry><config:config-item config:name=\"ViewId\" config:type=\"string\">view1</config:config-item><config:config-item-map-named config:name=\"Tables\">{tables}</config:config-item-map-named><config:config-item config:name=\"ActiveTable\" config:type=\"string\">{}</config:config-item></config:config-item-map-entry></config:config-item-map-indexed></config:config-item-set></office:settings></office:document-settings>",
        escape_text(&active)
    )
}

fn meta_xml(metadata: &Metadata) -> String {
    let mut body = String::from("<meta:generator>Letters</meta:generator>");
    let fields = [
        ("dc:title", &metadata.title),
        ("dc:subject", &metadata.subject),
        ("meta:initial-creator", &metadata.creator),
        ("meta:keyword", &metadata.keywords),
        ("dc:description", &metadata.description),
        ("dc:creator", &metadata.last_modified_by),
        ("meta:creation-date", &metadata.created),
        ("dc:date", &metadata.modified),
    ];
    for (name, value) in fields {
        if let Some(value) = value {
            body.push_str(&format!("<{name}>{}</{name}>", escape_text(value)));
        }
    }
    format!("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<office:document-meta {NAMESPACES}><office:meta>{body}</office:meta></office:document-meta>")
}

/// media types of carried parts, from the file's own manifest when it had one
fn media_type(book: &Workbook, name: &str) -> String {
    let manifest = kept(book, "manifest").unwrap_or("");
    let needle = format!("manifest:full-path=\"{name}\"");
    let found = manifest.find(&needle).and_then(|at| {
        let tag_start = manifest[..at].rfind('<')?;
        let tag_end = at + manifest[at..].find('>')?;
        let tag = &manifest[tag_start..tag_end];
        let type_at = tag.find("manifest:media-type=\"")? + 21;
        Some(tag[type_at..].split('"').next()?.to_string())
    });
    found.unwrap_or_else(|| match name.rsplit('.').next() {
        Some("xml") => "text/xml".into(),
        Some("png") => "image/png".into(),
        Some("jpg") | Some("jpeg") => "image/jpeg".into(),
        _ => String::new(),
    })
}

fn manifest_xml(book: &Workbook, parts: &[Part]) -> String {
    let mut out = String::from("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<manifest:manifest xmlns:manifest=\"urn:oasis:names:tc:opendocument:xmlns:manifest:1.0\" manifest:version=\"1.3\">");
    out.push_str(&format!("<manifest:file-entry manifest:full-path=\"/\" manifest:version=\"1.3\" manifest:media-type=\"{MIMETYPE}\"/>"));
    let mut folders: Vec<String> = Vec::new();
    for part in parts.iter().filter(|p| p.name != "mimetype") {
        // embedded objects (charts) are folders with their own media type
        if let Some((folder, _)) = part.name.split_once('/') {
            let entry = format!("{folder}/");
            if !folders.contains(&entry)
                && folder != "META-INF"
                && !media_type(book, &entry).is_empty()
            {
                out.push_str(&format!(
                    "<manifest:file-entry manifest:full-path=\"{}\" manifest:media-type=\"{}\"/>",
                    escape_attribute(&entry),
                    media_type(book, &entry)
                ));
                folders.push(entry);
            }
        }
        if part.name == "META-INF/manifest.xml" {
            continue;
        }
        out.push_str(&format!(
            "<manifest:file-entry manifest:full-path=\"{}\" manifest:media-type=\"{}\"/>",
            escape_attribute(&part.name),
            media_type(book, &part.name)
        ));
    }
    out.push_str("</manifest:manifest>");
    out
}

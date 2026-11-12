//! the parts a worksheet points to, read for the model: hyperlink
//! targets, notes (comments) with their vml drawing, tables, and a
//! summary of drawings and charts (which themselves stay carried).

use crate::error::SheetError;
use crate::model::carry::{fingerprint, CarriedComments, Relationship};
use crate::model::*;
use crate::package::resolve_target;

use super::read_extras::{read_chart, read_comments, read_drawing, read_table};
use super::read_sheet::SheetLinks;
use super::read_workbook::ReadState;
use super::relationships::*;

fn target_of(rels: &[Relationship], kind: &str, source: &str) -> Option<String> {
    rels.iter()
        .find(|rel| rel.kind == kind)
        .map(|rel| resolve_target(source, &rel.target))
}

pub(super) fn read_sheet_relations(
    state: &mut ReadState,
    part: &str,
    links: &SheetLinks,
    sheet: &mut Sheet,
) -> Result<(), SheetError> {
    let rels = state.relationships_of(part);
    for (link, id) in sheet.hyperlinks.iter_mut().zip(&links.hyperlink_ids) {
        if let Some(rel) = id
            .as_ref()
            .and_then(|id| rels.iter().find(|rel| rel.id == *id))
        {
            link.target = Some(rel.target.clone());
        }
    }
    read_sheet_comments(state, part, &rels, sheet)?;
    for id in &links.table_ids {
        let Some(rel) = rels.iter().find(|rel| rel.id == *id) else {
            continue;
        };
        let table_part = resolve_target(part, &rel.target);
        let Some(xml) = state.text(&table_part)? else {
            continue;
        };
        if let Some(table) = read_table(&xml) {
            let made = super::write_extras::table_xml(&table);
            sheet
                .carry
                .tables
                .push((table.id, table_part, xml, fingerprint(&made)));
            sheet.tables.push(table);
        }
    }
    read_sheet_drawings(state, part, &rels, sheet)?;
    // notes and tables get fresh relationships on every save; the rest
    // (drawings, printer settings, pivot tables, hyperlinks) keep theirs
    sheet.carry.relationships = rels
        .into_iter()
        .filter(|rel| ![COMMENTS_REL, TABLE_REL].contains(&rel.kind.as_str()))
        .filter(|rel| !(rel.kind == VML_REL && sheet.carry.comments.is_some()))
        .collect();
    Ok(())
}

fn read_sheet_comments(
    state: &mut ReadState,
    part: &str,
    rels: &[Relationship],
    sheet: &mut Sheet,
) -> Result<(), SheetError> {
    let Some(comments_part) = target_of(rels, COMMENTS_REL, part) else {
        return Ok(());
    };
    let Some(xml) = state.text(&comments_part)? else {
        return Ok(());
    };
    sheet.comments = read_comments(&xml);
    let vml_part = target_of(rels, VML_REL, part);
    let vml_xml = match &vml_part {
        Some(vml) => state.text(vml)?,
        None => None,
    };
    sheet.carry.comments = Some(CarriedComments {
        part: comments_part,
        xml,
        vml_part,
        vml_xml,
        fingerprint: comments_fingerprint(&sheet.comments),
    });
    Ok(())
}

pub fn comments_fingerprint(comments: &[Comment]) -> u64 {
    fingerprint(&serde_json::to_string(comments).unwrap_or_default())
}

fn read_sheet_drawings(
    state: &mut ReadState,
    part: &str,
    rels: &[Relationship],
    sheet: &mut Sheet,
) -> Result<(), SheetError> {
    for rel in rels.iter().filter(|rel| rel.kind == DRAWING_REL) {
        let drawing_part = resolve_target(part, &rel.target);
        // drawings are carried, so they are read without being marked used
        let Some(xml) = state.package.text(&drawing_part)? else {
            continue;
        };
        let drawing_rels = match state.package.text(&rels_part_for(&drawing_part))? {
            Some(xml) => read_relationships(&xml),
            None => Vec::new(),
        };
        for object in read_drawing(&xml) {
            sheet
                .drawings
                .push(drawing_object(state, &drawing_part, &drawing_rels, object)?);
        }
    }
    Ok(())
}

fn drawing_object(
    state: &ReadState,
    drawing_part: &str,
    rels: &[Relationship],
    object: super::read_extras::DrawnObject,
) -> Result<DrawingObject, SheetError> {
    let target = |id: &Option<String>| {
        id.as_ref()
            .and_then(|id| rels.iter().find(|rel| rel.id == *id))
            .map(|rel| resolve_target(drawing_part, &rel.target))
    };
    if let Some(chart_part) = target(&object.chart_id) {
        let chart = match state.package.text(&chart_part)? {
            Some(xml) => read_chart(&xml),
            None => read_chart(""),
        };
        return Ok(DrawingObject::Chart {
            anchor: object.anchor,
            chart,
            part: Some(chart_part),
        });
    }
    if let Some(picture) = target(&object.picture_id) {
        return Ok(DrawingObject::Image {
            anchor: object.anchor,
            name: object.name,
            part: picture,
        });
    }
    Ok(DrawingObject::Shape {
        anchor: object.anchor,
        name: object.name,
    })
}

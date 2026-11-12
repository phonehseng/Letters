//! what each format keeps of the freeform objects (shapes, groups,
//! connectors, floating tables, linked boxes, columns), and the closest
//! construct a writer chose where a format has no exact equivalent. every
//! finding is a plain sentence the person can read; the writers already do
//! what the sentence says.

use crate::model::*;

use super::{Issue, Location, Report};

/// which application the document is checked for.
#[derive(Clone, Copy, PartialEq)]
pub(super) enum Target {
    Word,
    LibreOffice,
}

fn issue(report: &mut Report, code: &'static str, block: usize, message: String) {
    report.issues.push(Issue {
        code,
        message,
        location: Some(Location {
            block,
            inline: None,
        }),
        fix: None,
    });
}

/// everything about drawings and layout that `target` would show
/// differently, or keep less exactly, than the model says.
pub(super) fn check(document: &Document, target: Target, report: &mut Report) {
    for (index, block) in document.blocks.iter().enumerate() {
        match block {
            Block::Shape(shape) => check_shape(shape, index, target, report),
            Block::Group(group) => check_group(group, index, target, report),
            Block::Table(table) => check_table(table, index, target, report),
            _ => {}
        }
    }
    check_sections(document, target, report);
    if target == Target::LibreOffice && document.carry.origin == Some(SourceFormat::Docx) {
        check_word_run_settings(document, report);
    }
}

fn check_shape(shape: &Shape, index: usize, target: Target, report: &mut Report) {
    if let Geometry::Preset { name, .. } = &shape.geometry {
        let (_, exact) = crate::shapes::nearest_odf_type(name);
        if target == Target::LibreOffice && !exact && !shape.is_line() {
            issue(report, "preset-without-odf-name", index, format!(
                "OpenDocument has no name for the “{name}” outline; it is written under LibreOffice's own name for Word's outline, which other OpenDocument programs may draw as a rectangle."
            ));
        }
    }
    if let Fill::Gradient(gradient) = &shape.fill {
        if target == Target::LibreOffice && gradient.stops.len() > 2 {
            issue(report, "gradient-stops", index, "This gradient has more than two colours; LibreOffice shows them all, other OpenDocument programs only the first and last.".into());
        }
        let opacities: Vec<Option<f64>> = gradient.stops.iter().map(|s| s.opacity).collect();
        if target == Target::LibreOffice && opacities.windows(2).any(|w| w[0] != w[1]) {
            issue(report, "gradient-opacity", index, "OpenDocument gives a gradient one see-through setting, so its colours' different transparencies become their average.".into());
        }
    }
    if let Some(stroke) = &shape.stroke {
        let has_ends = stroke.head.is_some() || stroke.tail.is_some();
        if target == Target::LibreOffice && has_ends {
            issue(report, "arrowhead-size", index, "Arrowheads are drawn in OpenDocument at a size worked out from the line's width (small, medium and large in Word), so they can look slightly bigger or smaller.".into());
        }
        if target == Target::LibreOffice && stroke.dash.starts_with("sys") {
            issue(
                report,
                "dash-pattern",
                index,
                "Word's short dash patterns are drawn with the nearest OpenDocument dash lengths."
                    .into(),
            );
        }
    }
    if let Some(connector) = &shape.connector {
        if connector.start.is_some() || connector.end.is_some() {
            issue(report, "connector-glue", index, "This connector is kept as a line; it no longer follows the objects it joins when they move.".into());
        }
    }
}

fn check_group(group: &Group, index: usize, target: Target, report: &mut Report) {
    let turned = group.rotation.is_some_and(|r| r.abs() > 1e-9)
        || group.flip_horizontal
        || group.flip_vertical;
    if target == Target::LibreOffice && turned {
        issue(report, "group-transform", index, "An OpenDocument group cannot be turned or mirrored itself, so each object in it is turned and mirrored around the group's centre instead; the look is the same.".into());
    }
    let has_text_box = group
        .children
        .iter()
        .any(|child| matches!(child, GroupChild::TextBox(_)));
    if target == Target::LibreOffice && has_text_box {
        issue(report, "group-text-box", index, "LibreOffice does not show a text frame inside a group, so text boxes in this group are written as rectangles holding the text.".into());
    }
    for child in &group.children {
        if let GroupChild::Shape(shape) = child {
            check_shape(shape, index, target, report);
        }
    }
}

fn check_table(table: &Table, index: usize, target: Target, report: &mut Report) {
    if target != Target::LibreOffice {
        return;
    }
    if table.position.is_some() {
        issue(report, "floating-table", index, "OpenDocument floats a table inside a frame without a border; text wraps around the frame's box rather than the table's own.".into());
    }
    if table.fixed_layout == Some(false) {
        issue(report, "table-autofit", index, "LibreOffice keeps table columns at their widths; Word would widen them to fit the text.".into());
    }
}

fn check_sections(document: &Document, target: Target, report: &mut Report) {
    let mut block = 0;
    for (index, section) in document.sections.iter().enumerate() {
        let widths: Twips = section
            .column_widths
            .iter()
            .map(|c| c.width + c.space)
            .sum();
        let last_space = section.column_widths.last().map(|c| c.space).unwrap_or(0);
        let fills = widths - last_space == section.page.text_width();
        if target == Target::LibreOffice && !section.column_widths.is_empty() && !fills {
            issue(report, "columns-fill-the-page", block, "OpenDocument columns always fill the width between the margins, so these columns are widened in proportion to do so.".into());
        }
        let previous = index.checked_sub(1).and_then(|i| document.sections.get(i));
        let page_changes = previous.is_some_and(|p| p.page != section.page);
        if target == Target::LibreOffice
            && section.start == SectionStart::Continuous
            && page_changes
        {
            issue(report, "continuous-new-page-size", block, "A section with another page size cannot continue on the same page in OpenDocument; it starts a new page.".into());
        }
        block = next_break(document, block);
    }
}

/// the block after the next section break from `from`.
fn next_break(document: &Document, from: usize) -> usize {
    document.blocks[from.min(document.blocks.len())..]
        .iter()
        .position(|b| matches!(b, Block::SectionBreak))
        .map(|at| from + at + 1)
        .unwrap_or(document.blocks.len())
}

/// word's character settings the model keeps only as word xml: kerning,
/// ligatures, character spacing and the like. they shape every line, and
/// opendocument does not get them.
fn check_word_run_settings(document: &Document, report: &mut Report) {
    let defaults = &document.styles.defaults.run.preserved;
    let shaping = defaults.iter().any(|xml| {
        xml.contains("w:kern") || xml.contains("ligatures") || xml.contains("w:spacing")
    });
    if shaping {
        report.issues.push(Issue {
            code: "word-character-shaping",
            message: "Word's kerning, ligatures or character spacing settings are not carried into OpenDocument, so lines can come out up to a few points longer or shorter.".into(),
            location: None,
            fix: None,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::super::{libreoffice_compatibility, word_compatibility};
    use crate::model::*;

    #[test]
    fn freeform_objects_report_what_opendocument_keeps_less_exactly() {
        let mut document = Document::new();
        document.carry.origin = Some(SourceFormat::Docx);
        let mut plaque = Shape::preset("plaque", 0, 0, 1000, 1000, "FFFFFF");
        plaque.fill = Fill::Gradient(Gradient {
            kind: GradientKind::Linear,
            angle: 0.0,
            stops: ["FF0000", "00FF00", "0000FF"]
                .iter()
                .enumerate()
                .map(|(i, c)| GradientStop {
                    position: i as f64 / 2.0,
                    color: c.to_string(),
                    opacity: None,
                })
                .collect(),
        });
        let mut connector = Shape::preset("straightConnector1", 0, 0, 1000, 0, "000000");
        connector.connector = Some(Connector {
            kind: "straight".into(),
            start: Some(ConnectionEnd {
                object: "1".into(),
                site: 0,
            }),
            end: None,
        });
        document.blocks = vec![
            Block::Shape(plaque),
            Block::Shape(connector),
            Block::text("x"),
        ];
        document.sections[0].column_widths = vec![
            ColumnWidth {
                width: 2000,
                space: 360,
            },
            ColumnWidth {
                width: 2000,
                space: 0,
            },
        ];
        let codes: Vec<&str> = libreoffice_compatibility(&document)
            .issues
            .iter()
            .map(|i| i.code)
            .collect();
        for expected in [
            "preset-without-odf-name",
            "gradient-stops",
            "connector-glue",
            "columns-fill-the-page",
        ] {
            assert!(
                codes.contains(&expected),
                "{expected} missing from {codes:?}"
            );
        }
        let word: Vec<&str> = word_compatibility(&document)
            .issues
            .iter()
            .map(|i| i.code)
            .collect();
        assert!(
            word.contains(&"connector-glue") && !word.contains(&"gradient-stops"),
            "{word:?}"
        );
    }
}

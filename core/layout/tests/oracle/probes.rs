//! probe documents: not documents anyone writes, but measurements. each
//! table probe puts one table on its own page, so the vertical rules an
//! application draws on that page are exactly that table's column edges.
//! `tests/oracle.rs` (`fit_table_probes`) prints, for every probe, the
//! columns each application chose next to the narrowest and natural
//! widths our engine measures for the same cells, and how candidate
//! autofit rules would have done.
//!
//! probes are exported by both applications (out/probes_word and
//! out/probes_lo) but never scored with the corpus.

use letters_office::model::*;

use super::corpus::{base_document, paragraph, sentence, Seeded};
use super::corpus2::Patch;

/// one probe document with the patches it needs after writing.
pub struct Probe {
    pub name: String,
    pub document: Document,
    pub patches: Vec<Patch>,
    /// what each table of the document holds: per table, per column, the
    /// text of each cell in that column (rows in order)
    pub tables: Vec<Vec<Vec<String>>>,
}

fn border() -> Border {
    Border {
        style: "single".into(),
        size: 4,
        color: Some("000000".into()),
    }
}

/// a table of the given cell texts (rows of cells), all borders, no widths.
fn table_of(rows: &[Vec<String>]) -> Table {
    Table {
        borders: Some(CellBorders {
            top: Some(border()),
            left: Some(border()),
            bottom: Some(border()),
            right: Some(border()),
            inside_horizontal: Some(border()),
            inside_vertical: Some(border()),
        }),
        rows: rows
            .iter()
            .map(|cells| TableRow {
                cells: cells
                    .iter()
                    .map(|text| TableCell::with_text(text))
                    .collect(),
                ..Default::default()
            })
            .collect(),
        ..Default::default()
    }
}

fn words(rng: &mut Seeded, count: usize) -> String {
    let text = sentence(rng, count);
    text.trim_end_matches('.').to_string()
}

/// the cell texts of every table probe: one table per page.
fn table_shapes(rng: &mut Seeded) -> Vec<Vec<Vec<String>>> {
    let mut shapes: Vec<Vec<Vec<String>>> = Vec::new();
    // one cell: its natural width, then the clamp at the margins
    for count in [1, 3, 8, 15, 30, 80] {
        shapes.push(vec![vec![words(rng, count)]]);
    }
    // two cells: a short column next to a growing one
    for count in [5, 12, 25, 60, 120] {
        shapes.push(vec![vec![words(rng, 2), words(rng, count)]]);
    }
    // two long columns of different lengths
    for (a, b) in [(30, 60), (40, 40), (20, 150), (100, 110)] {
        shapes.push(vec![vec![words(rng, a), words(rng, b)]]);
    }
    // three columns
    for (a, b, c) in [(1, 10, 30), (5, 40, 80), (60, 60, 60), (2, 2, 200)] {
        shapes.push(vec![vec![words(rng, a), words(rng, b), words(rng, c)]]);
    }
    // a long unbreakable word sets a column's narrowest width
    shapes.push(vec![vec![
        "internationalisationcharacteristics".to_string(),
        words(rng, 80),
    ]]);
    // several rows: the columns' widest cells decide
    shapes.push(vec![
        vec![words(rng, 1), words(rng, 20), words(rng, 3)],
        vec![words(rng, 4), words(rng, 2), words(rng, 40)],
        vec![words(rng, 12), words(rng, 6), words(rng, 1)],
    ]);
    // four and five long columns
    shapes.push(vec![(0..4).map(|i| words(rng, 20 + 20 * i)).collect()]);
    shapes.push(vec![(0..5).map(|i| words(rng, 10 + 15 * i)).collect()]);
    shapes
}

/// the same tables in calibri 11: with no cell margins (our writer's
/// plain files), and with word's usual 0.075 inch side margins.
pub fn probes() -> Vec<Probe> {
    let mut out = Vec::new();
    for (name, margins) in [("table_probe_plain", false), ("table_probe_margins", true)] {
        let mut rng = Seeded(4242);
        let shapes = table_shapes(&mut rng);
        let mut document = base_document("Calibri", 22, 0, LineSpacing::SINGLE);
        let mut tables = Vec::new();
        for (index, rows) in shapes.iter().enumerate() {
            if index > 0 {
                document.blocks.push(Block::PageBreak);
            }
            document
                .blocks
                .push(Block::Paragraph(paragraph(&format!("Probe {index}"))));
            document.blocks.push(Block::Table(table_of(rows)));
            // per column, the texts of its cells
            let columns = rows.iter().map(Vec::len).max().unwrap_or(0);
            tables.push(
                (0..columns)
                    .map(|c| rows.iter().filter_map(|r| r.get(c).cloned()).collect())
                    .collect(),
            );
        }
        let mut patches = Vec::new();
        if margins {
            patches.push(Patch::TableProperty {
                table: None,
                xml: "<w:tblCellMar><w:left w:w=\"108\" w:type=\"dxa\"/><w:right w:w=\"108\" w:type=\"dxa\"/></w:tblCellMar>".into(),
            });
        }
        out.push(Probe {
            name: name.to_string(),
            document,
            patches,
            tables,
        });
    }
    out
}

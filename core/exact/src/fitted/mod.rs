//! the fitted document font, the text path of positioned lines when word is
//! there: every word piece becomes one composite glyph whose components are
//! the source's own glyphs at their exact offsets, chosen by gsub rules, so
//! the docx keeps real text while word draws ink placed in font units
//! instead of its 1/600 in grid (`tools/fidelity/fitted-font`, REPORT.md:
//! 22,906 corpus glyphs within 0.0066 pt).
//!
//! - `pieces`: lines to runs to pieces (a word and its trailing spaces)
//! - `faces`: slots, composites, rules and the font copies
//! - `xml`: one run per piece with the feature of its slot
//! - `measure`: finds each piece's glyph in word's pdf by its outline
//!
//! one measuring export reads where word put every piece's origin; the
//! leftover error is baked into the composites' outlines (outlines never
//! move layout) and a second export is the checked result.

pub mod faces;
pub mod measure;
pub mod pieces;
pub mod xml;

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::docx::EmbeddedFont;
use crate::fonts::embed::font_key_from;
use crate::fonts::grid::GridFont;
use crate::fonts::rebuild::fnv1a;
use crate::hybrid::{ExportPdf, PageBody, PlannedPage};
use crate::pdf::{PageContent, PdfFile};
use crate::verify::{compare_glyphs, placed_glyphs, GlyphStats};
use crate::word::sizes::drawn_size;
use crate::ExactError;
use faces::RunIds;
use faces::{
    assign_slots, build_face, component_offsets, expected_origin, face_family, RunGlyphs, FEATURES,
};
use measure::{
    drawn_glyphs, expanded_glyphs, font_signature, index_by_signature, locate, FoundPiece,
};
use pieces::{line_items, make_pieces, split_runs, Drawn, Run};
use xml::{frame_paragraph, TextRun};

/// fsType bits that keep a font out of the fitted path: restricted (2),
/// which word ignores and substitutes, and preview and print (4), which
/// makes word wait on a prompt under automation (fitted-font probe a1).
const BLOCKING_FS_TYPE: u16 = 0x0006;

/// writes the docx from page bodies with these fonts embedded.
pub type WriteFitted<'a> =
    dyn FnMut(&[PageBody], &[EmbeddedFont], &Path) -> Result<(), ExactError> + 'a;

/// each font copy's bytes by its family name.
type FontBytes = HashMap<String, Vec<u8>>;

/// one positioned line of the fitted docx.
struct FittedLine {
    page: usize,
    x: f64,
    y: f64,
    runs: Vec<Run>,
    /// round one's frame, kept for a line whose font may not take the fitted path
    fallback: Option<String>,
    position: crate::report::FramePosition,
}

/// what the fitted text path did.
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FittedReport {
    pub runs: usize,
    pub pieces: usize,
    pub pieces_found_measuring: usize,
    pub pieces_found_final: usize,
    /// font copies per grid family
    pub faces: BTreeMap<String, usize>,
    pub lines_left_on_round_one: usize,
    pub exports: usize,
    pub word_seconds: f64,
    /// the verifier's numbers with every piece's glyph read as its components
    pub glyphs: GlyphStats,
}

/// what the fitted builder needs.
pub struct FittedContext<'c, 'f> {
    pub source: &'c [PageContent],
    pub export: &'c mut ExportPdf<'f>,
    pub write: &'c mut WriteFitted<'c>,
    pub work: &'c Path,
    pub docx: &'c Path,
}

/// the result: the docx is written and word's pdf of it is measured.
pub struct FittedOutcome {
    pub report: FittedReport,
    pub final_pdf: PathBuf,
}

/// the lines' runs and pieces; lines in a blocked font keep round one's frame.
fn plan_lines(
    pages: &[PlannedPage],
    grids: &HashMap<String, GridFont>,
    report: &mut FittedReport,
) -> Vec<FittedLine> {
    let mut lines = Vec::new();
    for page in pages {
        for planned in &page.lines {
            let mut runs = split_runs(line_items(planned));
            let blocked = runs.iter().any(|r| {
                grids
                    .get(&r.items[0].family)
                    .is_none_or(|g| g.fs_type & BLOCKING_FS_TYPE != 0)
            });
            if !blocked {
                for run in &mut runs {
                    let grid = &grids[&run.items[0].family];
                    make_pieces(run, grid.units_per_em as f64, drawn_size(grid.stored_size));
                }
            }
            report.lines_left_on_round_one += usize::from(blocked);
            lines.push(FittedLine {
                page: page.index,
                x: planned.line.x(),
                y: planned.line.y(),
                runs: if blocked { Vec::new() } else { runs },
                fallback: blocked.then(|| planned.locked_xml.clone()),
                position: planned.position,
            });
        }
    }
    lines
}

/// slots per grid family; returns the font copies each family needs.
fn assign_all(lines: &mut [FittedLine]) -> BTreeMap<String, usize> {
    let mut by_family: BTreeMap<String, Vec<&mut Run>> = BTreeMap::new();
    for line in lines.iter_mut() {
        for run in &mut line.runs {
            by_family
                .entry(run.items[0].family.clone())
                .or_default()
                .push(run);
        }
    }
    by_family
        .into_iter()
        .map(|(family, mut runs)| {
            let faces = assign_slots(&mut runs);
            (family, faces)
        })
        .collect()
}

/// every font copy of every family, written from the pieces as they stand;
/// sets each run's composite ids. returns the fonts to embed and the bytes
/// of each copy by family name.
fn build_fonts(
    lines: &mut [FittedLine],
    grids: &HashMap<String, GridFont>,
    faces: &BTreeMap<String, usize>,
    base_fonts: &[EmbeddedFont],
) -> Result<(Vec<EmbeddedFont>, FontBytes), ExactError> {
    let mut written: HashMap<String, Vec<u8>> = HashMap::new();
    for (family, &count) in faces {
        let grid = &grids[family];
        for face in 0..count {
            let (bytes, ids) = face_font(lines, grid, family, face)?;
            assign_ids(lines, family, face, ids);
            written.insert(face_family(family, face), bytes);
        }
    }
    let mut fonts: Vec<EmbeddedFont> = base_fonts
        .iter()
        .map(|font| EmbeddedFont {
            bytes: written
                .get(&font.family)
                .cloned()
                .unwrap_or_else(|| font.bytes.clone()),
            ..font.clone()
        })
        .collect();
    for (family, bytes) in &written {
        if !fonts.iter().any(|f| &f.family == family) {
            fonts.push(EmbeddedFont {
                family: family.clone(),
                bytes: bytes.clone(),
                font_key: font_key_from(fnv1a(family.as_bytes())),
            });
        }
    }
    Ok((fonts, written))
}

/// one copy of a family's font with the composites of its runs.
fn face_font(
    lines: &[FittedLine],
    grid: &GridFont,
    family: &str,
    face: usize,
) -> Result<(Vec<u8>, RunIds), ExactError> {
    let mut entries = Vec::new();
    for line in lines {
        for (index, run) in line.runs.iter().enumerate() {
            if run.items[0].family != family || run.face != face || run.pieces.is_empty() {
                continue;
            }
            entries.push(RunGlyphs {
                run,
                line_x: line.x,
                line_y: line.y,
                next_run_along: line.runs.get(index + 1).map(|r| r.items[0].along),
            });
        }
    }
    build_face(grid, face, &entries).map_err(ExactError::WordFailed)
}

/// hands each run of this family and copy its composite ids, in the order they were built.
fn assign_ids(lines: &mut [FittedLine], family: &str, face: usize, ids: Vec<Vec<Option<u16>>>) {
    let mut ids = ids.into_iter();
    for line in lines.iter_mut() {
        for run in &mut line.runs {
            if run.items[0].family == family && run.face == face && !run.pieces.is_empty() {
                run.ids = ids.next().unwrap_or_default();
            }
        }
    }
}

/// a `w:r` of this run's style holding `text`.
fn text_run(run: &Run, text: String, feature: Option<&'static [u8; 4]>) -> TextRun {
    let first = &run.items[0];
    TextRun {
        text,
        family: face_family(&first.family, run.face),
        half_points: first.half_points,
        color: first.color.clone(),
        hidden: first.hidden,
        feature,
    }
}

/// the runs of one line as `w:r` elements: one per piece with a rule, the
/// rest as plain text.
fn line_runs(line: &FittedLine) -> Vec<TextRun> {
    let mut runs = Vec::new();
    for run in &line.runs {
        if run.pieces.is_empty() {
            runs.push(text_run(run, run.text(), None));
            continue;
        }
        for (piece, id) in run.pieces.iter().zip(&run.ids) {
            let feature = id.map(|_| FEATURES[run.feature]);
            runs.push(text_run(run, piece.text(), feature));
        }
    }
    runs
}

/// the page bodies: fitted lines in their frames, fallbacks as round one wrote them.
fn bodies(pages: &[PlannedPage], lines: &[FittedLine]) -> Vec<PageBody> {
    pages
        .iter()
        .map(|page| PageBody {
            items: lines
                .iter()
                .filter(|l| l.page == page.index)
                .map(|line| match &line.fallback {
                    Some(xml) => xml.clone(),
                    None => frame_paragraph(&line_runs(line), &line.position),
                })
                .collect(),
        })
        .collect()
}

/// writes, exports and reads word's pdf.
fn export_pass(
    pages: &[PlannedPage],
    lines: &[FittedLine],
    fonts: &[EmbeddedFont],
    path: &Path,
    context: &mut FittedContext,
    report: &mut FittedReport,
) -> Result<(PathBuf, Vec<PageContent>), ExactError> {
    (context.write)(&bodies(pages, lines), fonts, path)?;
    let pdf = path.with_extension("word.pdf");
    report.word_seconds += (context.export)(path, &pdf)?;
    report.exports += 1;
    let word_pages = PdfFile::open(&pdf)?.read_pages();
    Ok((pdf, word_pages))
}

/// every piece found in word's pdf: sets where it was drawn, and returns
/// its components' ink positions as the composites now place them.
fn locate_pieces(
    lines: &mut [FittedLine],
    word_pages: &[PageContent],
    written: &HashMap<String, Vec<u8>>,
    grids: &HashMap<String, GridFont>,
) -> Vec<FoundPiece> {
    let drawn = drawn_glyphs(word_pages);
    let index = index_by_signature(&drawn);
    let mut found = Vec::new();
    for line in lines.iter_mut() {
        let (page, x, y) = (line.page, line.x, line.y);
        for run in &mut line.runs {
            let family = run.items[0].family.clone();
            let grid = &grids[&family];
            let font = &written[&face_family(&family, run.face)];
            for (piece, id) in run.pieces.iter_mut().zip(run.ids.clone()) {
                let Some(signature) = id.and_then(|id| font_signature(font, id)) else {
                    continue;
                };
                let expected = piece
                    .drawn
                    .map(|d| d.origin)
                    .unwrap_or_else(|| expected_origin(piece, x, y));
                let Some(glyph) = locate(&drawn, &index, &signature, page, expected) else {
                    continue;
                };
                let offsets = component_offsets(piece, grid, x, y);
                let scale = glyph.size / grid.units_per_em as f64;
                let inked = piece.items.iter().filter(|i| i.inked());
                let components = inked
                    .zip(offsets)
                    .map(|(item, (dx, dy))| {
                        (
                            item.char.clone(),
                            glyph.x + dx as f64 * scale,
                            glyph.y + dy as f64 * scale,
                        )
                    })
                    .collect();
                found.push(FoundPiece {
                    drawn_index: glyph.index,
                    components,
                });
                piece.drawn = Some(Drawn {
                    origin: (glyph.x, glyph.y),
                    size: glyph.size,
                });
            }
        }
    }
    found
}

/// the fitted docx: pieces planned, one measuring export, the residual
/// baked into the outlines, and the final export measured.
pub fn build_fitted(
    pages: &[PlannedPage],
    grids: &HashMap<String, GridFont>,
    base_fonts: &[EmbeddedFont],
    context: &mut FittedContext,
) -> Result<FittedOutcome, ExactError> {
    let mut report = FittedReport::default();
    let mut lines = plan_lines(pages, grids, &mut report);
    report.runs = lines.iter().map(|l| l.runs.len()).sum();
    report.pieces = lines
        .iter()
        .flat_map(|l| &l.runs)
        .map(|r| r.pieces.len())
        .sum();
    let faces = assign_all(&mut lines);
    report.faces = faces.clone();
    let measuring = context.work.join("fitted_measure.docx");
    let (fonts, written) = build_fonts(&mut lines, grids, &faces, base_fonts)?;
    let (_, word_pages) = export_pass(pages, &lines, &fonts, &measuring, context, &mut report)?;
    report.pieces_found_measuring = locate_pieces(&mut lines, &word_pages, &written, grids).len();
    // the residual goes into the outlines: only offsets change, never an advance
    let (fonts, written) = build_fonts(&mut lines, grids, &faces, base_fonts)?;
    let docx = context.docx.to_path_buf();
    let (final_pdf, word_pages) = export_pass(pages, &lines, &fonts, &docx, context, &mut report)?;
    let found = locate_pieces(&mut lines, &word_pages, &written, grids);
    report.pieces_found_final = found.len();
    report.glyphs = compare_glyphs(
        &placed_glyphs(context.source),
        &expanded_glyphs(&word_pages, &found),
    );
    Ok(FittedOutcome { report, final_pdf })
}

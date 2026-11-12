//! probes for the hybrid mode: where word puts the lines of real
//! paragraphs (several lines, spacing before and after, indents), stacked
//! in one frame or flowing in the body. the measurements behind the rules
//! in `src/hybrid/anchoring.rs` come from here:
//!
//!     cargo test -p letters-exact --release --test hybrid_probe -- --ignored --nocapture
//!
//! each line holds one probe glyph, so word's pdf gives every line's start
//! pixel and baseline pixel exactly (recipe 6.3).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use letters_exact::bridge::WordBridge;
use letters_exact::calibration::WordModel;
use letters_exact::docx::xml::{
    page_break_paragraph, runs_xml, section_properties, LAYOUT_NEUTRAL,
};
use letters_exact::docx::{EmbeddedFont, Package};
use letters_exact::fonts::embed::font_key_from;
use letters_exact::fonts::outline::ContourPoint;
use letters_exact::fonts::truetype::{write_truetype, FontGlyph, FontSpec};
use letters_exact::pdf::PdfFile;
use letters_exact::plan::glyphs::CharRun;
use letters_exact::verify::raw::read_text_objects;
use letters_exact::word::writer_model::{decode_x, decode_y, page_pixels, twips_pixel, StepTable};
use letters_exact::CancelFlag;

const FAMILY: &str = "Letters Hybrid Probe";
const X0: i64 = 1440;
const Y0: i64 = 1000;

fn probe_font() -> EmbeddedFont {
    let point = |x, y| ContourPoint {
        x,
        y,
        on_curve: true,
    };
    let glyphs = vec![
        FontGlyph {
            contours: Vec::new(),
            advance: 600,
        },
        FontGlyph {
            contours: Vec::new(),
            advance: 300,
        },
        FontGlyph {
            contours: vec![vec![
                point(50, 0),
                point(50, 450),
                point(550, 450),
                point(550, 0),
            ]],
            advance: 600,
        },
    ];
    let cmap = BTreeMap::from([(0x20, 1), (u32::from(b'n'), 2)]);
    EmbeddedFont {
        family: FAMILY.into(),
        bytes: write_truetype(&FontSpec {
            family: FAMILY,
            units_per_em: 1200,
            glyphs: &glyphs,
            cmap: &cmap,
            ascent: 960,
            descent: -240,
            fs_type: 0,
        }),
        font_key: font_key_from(0x5EED),
    }
}

/// one probe paragraph.
#[derive(Clone, Copy, Debug)]
struct Para {
    line: i64,
    before: i64,
    after: i64,
    left: i64,
    first: i64,
    lines: usize,
}

fn paragraphs(seed: u64, count: usize) -> Vec<Para> {
    let mut state = seed;
    let mut pick = |values: &[i64]| {
        state = state
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        values[((state >> 33) % values.len() as u64) as usize]
    };
    (0..count)
        .map(|_| {
            let left = pick(&[0, 7, 133, 359, 721]);
            let first = pick(&[0, 19, 360, -133]).max(-left);
            Para {
                line: pick(&[228, 240, 250, 271, 276, 283, 300, 229, 233]),
                before: pick(&[0, 7, 13, 31, 120]),
                after: pick(&[0, 5, 11, 60]),
                left,
                first,
                lines: 3,
            }
        })
        .collect()
}

fn run() -> String {
    runs_xml(&[CharRun {
        char: "n".into(),
        family: FAMILY.into(),
        half_points: 24,
        color: "000000".into(),
        spacing_twips: 0,
        hidden: false,
    }])
}

fn paragraph_xml(para: &Para, frame: Option<&str>) -> String {
    let mut runs = String::new();
    for index in 0..para.lines {
        if index > 0 {
            runs.push_str("<w:r><w:br/></w:r>");
        }
        runs.push_str(&run());
    }
    let first = if para.first >= 0 {
        format!(r#"w:firstLine="{}""#, para.first)
    } else {
        format!(r#"w:hanging="{}""#, -para.first)
    };
    format!(
        r#"<w:p><w:pPr>{}{LAYOUT_NEUTRAL}<w:spacing w:before="{}" w:after="{}" w:line="{}" w:lineRule="exact"/><w:ind w:left="{}" w:right="0" {first}/></w:pPr>{runs}</w:p>"#,
        frame.unwrap_or(""),
        para.before,
        para.after,
        para.line,
        para.left,
    )
}

/// predicted (x pixel, baseline pixel) of every line: line tops add up in
/// twips, then each line top is a frame top (recipe 6.1 and 6.6).
fn predict(paras: &[Para], model: &WordModel) -> Vec<(i64, i64)> {
    let mut top = Y0;
    let mut lines = Vec::new();
    let mut after_previous = 0;
    for para in paras {
        // word keeps the larger of the space after and the space before
        top += para.before.max(after_previous);
        for index in 0..para.lines {
            let indent = para.left + if index == 0 { para.first } else { 0 };
            let baseline = twips_pixel(top) + model.baseline_offset(para.line, top);
            lines.push((twips_pixel(X0) + (5 * indent + 5).div_euclid(12), baseline));
            top += para.line;
        }
        after_previous = para.after;
    }
    lines
}

fn out_dir() -> PathBuf {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("target-exact")
        .join("word-tests")
        .join("hybrid-probe");
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn write(path: &Path, body: Vec<String>, final_section: String) {
    let mut package = Package::default();
    package.body = body;
    package.final_section = final_section;
    package.fonts.push(probe_font());
    std::fs::write(path, package.to_bytes().unwrap()).unwrap();
}

/// the body-flow section: margins put the text area where the frames are.
fn body_section() -> String {
    section_properties(612.0, 792.0).replace(
        r#"<w:pgMar w:top="0" w:right="0" w:bottom="0" w:left="0""#,
        &format!(r#"<w:pgMar w:top="{Y0}" w:right="1440" w:bottom="0" w:left="{X0}""#),
    )
}

fn letter_tables(model: &mut WordModel) -> (StepTable, StepTable) {
    use letters_exact::calibration::Axis;
    (
        model.table(Axis::X, page_pixels(612.0), false),
        model.table(Axis::Y, page_pixels(792.0), false),
    )
}

/// a docx and the pdf word exports it to.
type ExportJob = (PathBuf, PathBuf);
/// a probe document's name and its paragraphs.
type ProbeSet = (String, Vec<Para>);

/// (page, x pixel, baseline pixel) of every probe glyph in word's pdf, in order.
fn produced_positions(pdf: &Path, model: &mut WordModel) -> Vec<(usize, i64, i64)> {
    let (x_table, y_table) = letter_tables(model);
    let height = page_pixels(792.0);
    read_text_objects(&PdfFile::open(pdf).unwrap())
        .iter()
        .filter(|o| o.codes == [u32::from(b'n')])
        .map(|o| {
            (
                o.page,
                decode_x(o.x(), &x_table).unwrap_or(-1),
                decode_y(o.y(), &y_table, height).unwrap_or(-1),
            )
        })
        .collect()
}

/// the probe documents: per seed, the same paragraphs stacked in one frame
/// and flowing in the body. returns the export jobs and each one's name and paragraphs.
fn probe_documents(dir: &Path) -> (Vec<ExportJob>, Vec<ProbeSet>) {
    let frame = format!(
        r#"<w:framePr w:w="7000" w:wrap="through" w:vAnchor="page" w:hAnchor="page" w:x="{X0}" w:y="{Y0}"/>"#
    );
    let mut jobs = Vec::new();
    let mut sets = Vec::new();
    for seed in [3u64, 11, 29, 41, 57, 73] {
        let paras = paragraphs(seed, 13);
        let stacked: Vec<String> = paras
            .iter()
            .map(|p| paragraph_xml(p, Some(&frame)))
            .collect();
        let flowing: Vec<String> = paras.iter().map(|p| paragraph_xml(p, None)).collect();
        for (name, body, section) in [
            ("stacked", stacked, section_properties(612.0, 792.0)),
            ("body", flowing, body_section()),
        ] {
            let docx = dir.join(format!("{name}-{seed}.docx"));
            write(&docx, body, section);
            jobs.push((docx.clone(), docx.with_extension("word.pdf")));
            sets.push((format!("{name}-{seed}"), paras.clone()));
        }
    }
    (jobs, sets)
}

/// prints how many line starts and baselines differ from the model, and
/// keeps the raw numbers next to the pdf for fitting.
fn report_probe(
    name: &str,
    pdf: &Path,
    paras: &[Para],
    produced: &[(usize, i64, i64)],
    predicted: &[(i64, i64)],
) {
    let record = serde_json::json!({
        "paragraphs": paras.iter().map(|p| [p.line, p.before, p.after, p.left, p.first, p.lines as i64]).collect::<Vec<_>>(),
        "produced": produced.iter().map(|(_, x, y)| (x, y)).collect::<Vec<_>>(),
    });
    std::fs::write(pdf.with_extension("json"), record.to_string()).unwrap();
    let x_misses = produced
        .iter()
        .zip(predicted)
        .filter(|(a, b)| a.1 != b.0)
        .count();
    let y_misses: Vec<(usize, i64)> = produced
        .iter()
        .zip(predicted)
        .enumerate()
        .filter(|(_, (a, b))| a.2 != b.1)
        .map(|(i, (a, b))| (i, a.2 - b.1))
        .collect();
    println!(
        "{name}: {} lines produced, {} predicted; x misses {x_misses}; y misses {} {:?}",
        produced.len(),
        predicted.len(),
        y_misses.len(),
        &y_misses[..y_misses.len().min(12)]
    );
}

#[test]
#[ignore = "starts microsoft word"]
fn where_word_puts_the_lines_of_real_paragraphs() {
    let dir = out_dir();
    let (jobs, sets) = probe_documents(&dir);
    let bridge = WordBridge {
        seconds_per_document: 60,
        work_dir: dir.join("word"),
    };
    let run = bridge.export(&jobs, &CancelFlag::new()).unwrap();
    println!("word process {:?}, {:.1} s", run.word_process, run.seconds);
    let mut model = WordModel::built_in();
    for ((_, pdf), (name, paras)) in jobs.iter().zip(&sets) {
        let produced = produced_positions(pdf, &mut model);
        let predicted = predict(paras, &model);
        report_probe(name, pdf, paras, &produced, &predicted);
    }
}

/// one three-line paragraph per frame for every line height from 200 to
/// 330 twips, line tops kept off pixel edges: the body and (height, top) of each.
fn following_line_frames() -> (Vec<String>, Vec<(i64, i64)>) {
    let mut body = Vec::new();
    let mut frames = Vec::new();
    let per_page = 4 * 13;
    for (index, line) in (200..=330).enumerate() {
        if index > 0 && index % per_page == 0 {
            body.push(page_break_paragraph());
        }
        let slot = index % per_page;
        let x = X0 + (slot % 4) as i64 * 2600;
        let mut top = 800 + (slot / 4) as i64 * 1150;
        while (0..3).any(|k| letters_exact::hybrid::rules::on_pixel_edge(top + k * line)) {
            top += 1;
        }
        let frame = format!(
            r#"<w:framePr w:w="2400" w:wrap="through" w:vAnchor="page" w:hAnchor="page" w:x="{x}" w:y="{top}"/>"#
        );
        let para = Para {
            line,
            before: 0,
            after: 0,
            left: 0,
            first: 0,
            lines: 3,
        };
        body.push(paragraph_xml(&para, Some(&frame)));
        frames.push((line, top));
    }
    (body, frames)
}

/// lines whose baseline differs from the rule, per line of the paragraph.
fn following_line_misses(
    name: &str,
    rules: &WordModel,
    frames: &[(i64, i64)],
    produced: &[(usize, i64, i64)],
) -> [usize; 3] {
    let mut misses = [0; 3];
    for (number, (line, top)) in frames.iter().enumerate() {
        for (k, miss) in misses.iter_mut().enumerate() {
            let baseline = produced[3 * number + k].2;
            let line_top = top + k as i64 * line;
            let predicted = twips_pixel(line_top) + rules.baseline_offset(*line, line_top);
            if baseline != predicted {
                *miss += 1;
                println!(
                    "    {name}: line {line}, top {top}, line {k}: {baseline} against {predicted}"
                );
            }
        }
    }
    misses
}

/// the baselines of the lines after the first, for every exact line height
/// from 200 to 330 twips, against the closed rule and against each height's
/// measured table (`calibration::measure_baselines`, used by
/// `src/hybrid/vertical.rs`). writes `following-lines.json`.
#[test]
#[ignore = "starts microsoft word"]
fn the_lines_after_the_first_of_every_line_height() {
    let dir = out_dir();
    let (body, frames) = following_line_frames();
    let docx = dir.join("following-lines.docx");
    write(&docx, body, section_properties(612.0, 792.0));
    let pdf = docx.with_extension("word.pdf");
    let bridge = WordBridge {
        seconds_per_document: 120,
        work_dir: dir.join("word"),
    };
    let run = bridge
        .export(&[(docx.clone(), pdf.clone())], &CancelFlag::new())
        .unwrap();
    println!("word process {:?}, {:.1} s", run.word_process, run.seconds);
    let mut model = WordModel::built_in();
    let closed_rule = model.clone();
    // the frame tops' tables of every height, measured as calibration does (recipe 9)
    let heights: Vec<i64> = frames.iter().map(|(line, _)| *line).collect();
    letters_exact::calibration::measure_baselines(
        &mut model,
        &heights,
        &bridge,
        &dir.join("tables"),
        &CancelFlag::new(),
    )
    .unwrap();
    model.save(&dir.join("tables")).unwrap();
    let produced = produced_positions(&pdf, &mut model);
    for (name, rules) in [("closed rule", &closed_rule), ("measured tables", &model)] {
        let misses = following_line_misses(name, rules, &frames, &produced);
        println!(
            "{name}: lines off per line of the paragraph {misses:?} of {} each",
            frames.len()
        );
    }
    let baselines: Vec<(usize, i64)> = produced.iter().map(|(page, _, y)| (*page, *y)).collect();
    let record = serde_json::json!({ "frames": frames, "baselines": baselines });
    std::fs::write(dir.join("following-lines.json"), record.to_string()).unwrap();
}

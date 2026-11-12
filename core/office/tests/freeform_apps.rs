//! the freeform round-trip suites against microsoft word and libreoffice.
//! ignored by default because they need both installed; run with
//!
//!     cargo test -p letters-office --test freeform_apps -- --ignored --nocapture --test-threads 1
//!
//! the corpus is made by the applications themselves (word through its
//! object model, libreoffice from flat odt it saves as .odt), so every
//! file is what that application writes. each suite renders through the
//! applications' own pdf export and measures glyph positions, object
//! rectangles and page counts. results are printed and written to
//! `%TEMP%\letters-freeform\results`; ROUNDTRIP_REPORT.md is made from them.

mod support;

use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::time::Instant;

use letters_office::bridge::WORD_FORMAT_ODT;
use letters_office::{read_docx, read_odt, write_docx, write_odt};
use support::apps::*;

/// pdf exports of the same file differ by nothing; anything above this is
/// a real move (word prints positions to 1/3000 inch, 0.024 pt).
const EXPORT_NOISE_PT: f64 = 0.03;

fn timed<T>(label: &str, work: impl FnOnce() -> T) -> T {
    let started = Instant::now();
    let result = work();
    eprintln!("{label}: {:.1} s", started.elapsed().as_secs_f64());
    result
}

fn write_results(name: &str, text: &str) {
    let path = work_dir("results").join(name);
    std::fs::write(&path, text).unwrap();
    eprintln!("results in {}", path.display());
}

/// named corpus files.
type Files = Vec<(String, PathBuf)>;

/// the corpus: word's docx files and libreoffice's odt files.
fn corpus() -> (Files, Files) {
    let word = timed("word makes its corpus", word_corpus);
    let libreoffice = timed("libreoffice makes its corpus", libreoffice_corpus);
    (word, libreoffice)
}

/// opens a file with letters and saves it in the same format, untouched.
fn resave(path: &Path, out_dir: &Path) -> PathBuf {
    let bytes = std::fs::read(path).unwrap();
    let docx = path.extension().is_some_and(|e| e == "docx");
    let saved = if docx {
        write_docx(&read_docx(&bytes).unwrap()).unwrap()
    } else {
        write_odt(&read_odt(&bytes).unwrap()).unwrap()
    };
    let target = out_dir.join(path.file_name().unwrap());
    std::fs::write(&target, saved).unwrap();
    target
}

/// one row of a results table.
fn row(table: &mut String, document: &str, renderer: &str, difference: Option<&Difference>) {
    match difference {
        Some(d) => {
            let _ = writeln!(
                table,
                "| {document} | {renderer} | {}/{} | {}/{} | {:.3} | {:.4} | {}/{} | {:.3} | {:.4} | {} |",
                d.pages.0,
                d.pages.1,
                d.glyphs.glyphs,
                d.glyphs.source_glyphs,
                d.glyphs.max_pt,
                d.glyphs.mean_pt,
                d.objects_matched,
                d.objects.0,
                d.object_max_pt,
                d.object_mean_pt,
                if d.identical(EXPORT_NOISE_PT) { "identical" } else { "differs" }
            );
        }
        None => {
            let _ = writeln!(
                table,
                "| {document} | {renderer} | export failed | | | | | | | |"
            );
        }
    }
}

const TABLE_HEAD: &str = "| document | rendered by | pages | glyphs matched | glyph max pt | glyph mean pt | objects matched | object max pt | object mean pt | verdict |\n|---|---|---|---|---|---|---|---|---|---|\n";

/// pairs of (first, second) pdfs compared, as differences.
fn differences(first: &[Option<Vec<u8>>], second: &[Option<Vec<u8>>]) -> Vec<Option<Difference>> {
    first
        .iter()
        .zip(second)
        .map(|(a, b)| {
            Some(compare(
                &read_rendering(a.as_ref()?),
                &read_rendering(b.as_ref()?),
            ))
        })
        .collect()
}

#[test]
#[ignore = "needs Microsoft Word and LibreOffice"]
fn nothing_moves_when_a_document_is_opened_and_saved() {
    let before = office_processes();
    let (word, libreoffice) = corpus();
    let out = work_dir("nothing-moves");
    let originals: Vec<(String, PathBuf)> = word.into_iter().chain(libreoffice).collect();
    let resaved: Vec<PathBuf> = timed("letters opens and saves", || {
        originals.iter().map(|(_, p)| resave(p, &out)).collect()
    });
    let first: Vec<PathBuf> = originals.iter().map(|(_, p)| p.clone()).collect();
    let all: Vec<PathBuf> = first.iter().chain(&resaved).cloned().collect();
    let word_pdfs = timed("word renders", || word_pdfs(&all, &out.join("word")));
    let lo_pdfs = timed("libreoffice renders", || {
        libreoffice_pdfs(&all, &out.join("libreoffice"))
    });
    let half = first.len();
    let by_word = differences(&word_pdfs[..half], &word_pdfs[half..]);
    let by_libreoffice = differences(&lo_pdfs[..half], &lo_pdfs[half..]);
    let mut table = String::from(TABLE_HEAD);
    let mut moved = Vec::new();
    for (index, (name, _)) in originals.iter().enumerate() {
        for (renderer, found) in [
            ("Word", &by_word[index]),
            ("LibreOffice", &by_libreoffice[index]),
        ] {
            row(&mut table, name, renderer, found.as_ref());
            if !found.as_ref().is_some_and(|d| d.identical(EXPORT_NOISE_PT)) {
                moved.push(format!(
                    "{name} in {renderer}: {}",
                    found
                        .as_ref()
                        .map(Difference::summary)
                        .unwrap_or_else(|| "export failed".into())
                ));
            }
        }
    }
    eprintln!("{table}");
    write_results("nothing_moves.md", &table);
    assert_none_left(&before);
    assert!(moved.is_empty(), "moved:\n{}", moved.join("\n"));
}

/// a document through letters into the other format and back.
fn there_and_back(path: &Path, out: &Path) -> PathBuf {
    let bytes = std::fs::read(path).unwrap();
    let stem = path.file_stem().unwrap().to_string_lossy().into_owned();
    if path.extension().is_some_and(|e| e == "docx") {
        let odt = write_odt(&read_docx(&bytes).unwrap()).unwrap();
        std::fs::write(out.join(format!("{stem}.letters.odt")), &odt).unwrap();
        let docx = write_docx(&read_odt(&odt).unwrap()).unwrap();
        let target = out.join(format!("{stem}.back.docx"));
        std::fs::write(&target, docx).unwrap();
        target
    } else {
        let docx = write_docx(&read_odt(&bytes).unwrap()).unwrap();
        std::fs::write(out.join(format!("{stem}.letters.docx")), &docx).unwrap();
        let odt = write_odt(&read_docx(&docx).unwrap()).unwrap();
        let target = out.join(format!("{stem}.back.odt"));
        std::fs::write(&target, odt).unwrap();
        target
    }
}

#[test]
#[ignore = "needs Microsoft Word and LibreOffice"]
fn back_and_forth_between_formats_and_applications() {
    let before = office_processes();
    let (word, libreoffice) = corpus();
    let out = work_dir("back-and-forth");
    let word_files: Vec<PathBuf> = word.iter().map(|(_, p)| p.clone()).collect();
    let lo_files: Vec<PathBuf> = libreoffice.iter().map(|(_, p)| p.clone()).collect();
    // letters: docx -> odt -> docx (word renders both ends), odt -> docx -> odt
    // (libreoffice renders both ends)
    let word_back: Vec<PathBuf> = timed("letters converts there and back", || {
        word_files
            .iter()
            .chain(&lo_files)
            .map(|p| there_and_back(p, &out))
            .collect()
    });
    let (docx_back, odt_back) = word_back.split_at(word_files.len());
    // the applications' own conversion, the baseline to beat
    let word_odt = timed("word saves as odt", || {
        word_saves(&word_files, WORD_FORMAT_ODT, "odt", &out.join("word-odt"))
    });
    let lo_docx = timed("libreoffice saves as docx", || {
        libreoffice_outputs(
            &lo_files,
            "docx:MS Word 2007 XML",
            "docx",
            &out.join("lo-docx"),
        )
    });
    let word_baseline: Vec<PathBuf> = word_odt.iter().flatten().cloned().collect();
    let lo_baseline: Vec<PathBuf> = lo_docx.iter().flatten().cloned().collect();
    assert_eq!(
        word_baseline.len(),
        word_files.len(),
        "word saved every file as odt"
    );
    assert_eq!(
        lo_baseline.len(),
        lo_files.len(),
        "libreoffice saved every file as docx"
    );
    let word_inputs: Vec<PathBuf> = word_files
        .iter()
        .chain(docx_back)
        .chain(&word_baseline)
        .cloned()
        .collect();
    let word_rendered = timed("word renders", || {
        word_pdfs(&word_inputs, &out.join("word-pdf"))
    });
    let lo_inputs: Vec<PathBuf> = lo_files
        .iter()
        .chain(odt_back)
        .chain(&lo_baseline)
        .cloned()
        .collect();
    let lo_rendered = timed("libreoffice renders", || {
        libreoffice_pdfs(&lo_inputs, &out.join("lo-pdf"))
    });
    let n = word_files.len();
    let letters_word = differences(&word_rendered[..n], &word_rendered[n..2 * n]);
    let baseline_word = differences(&word_rendered[..n], &word_rendered[2 * n..]);
    let m = lo_files.len();
    let letters_lo = differences(&lo_rendered[..m], &lo_rendered[m..2 * m]);
    let baseline_lo = differences(&lo_rendered[..m], &lo_rendered[2 * m..]);
    let mut table = String::from(TABLE_HEAD);
    for (index, (name, _)) in word.iter().enumerate() {
        row(
            &mut table,
            &format!("{name}: Letters docx→odt→docx"),
            "Word",
            letters_word[index].as_ref(),
        );
        row(
            &mut table,
            &format!("{name}: Word saves as odt"),
            "Word",
            baseline_word[index].as_ref(),
        );
    }
    for (index, (name, _)) in libreoffice.iter().enumerate() {
        row(
            &mut table,
            &format!("{name}: Letters odt→docx→odt"),
            "LibreOffice",
            letters_lo[index].as_ref(),
        );
        row(
            &mut table,
            &format!("{name}: LibreOffice saves as docx"),
            "LibreOffice",
            baseline_lo[index].as_ref(),
        );
    }
    eprintln!("{table}");
    write_results("back_and_forth.md", &table);
    assert_none_left(&before);
}

/// a canonical form of a part's xml: element names, attributes sorted,
/// text; without revision ids, paragraph ids, namespace declarations (a
/// rewritten root declares more) and the `xml:space` switch, which change
/// nothing that is shown.
fn canonical(xml: &str) -> Vec<String> {
    fn walk(element: &letters_office::xml::Element, out: &mut Vec<String>) {
        let mut attributes: Vec<String> = element
            .attributes
            .iter()
            .filter(|(name, _)| {
                !(name.starts_with("w:rsid")
                    || name.starts_with("w14:")
                    || name.starts_with("xmlns:")
                    || name == "xml:space")
            })
            .map(|(name, value)| format!("{name}={value}"))
            .collect();
        attributes.sort();
        out.push(format!("<{} {}>", element.name, attributes.join(" ")));
        for node in &element.children {
            match node {
                letters_office::xml::Node::Element(child) => walk(child, out),
                letters_office::xml::Node::Text(text)
                    if !text.trim().is_empty() || element.name == "w:t" =>
                {
                    out.push(text.clone())
                }
                _ => {}
            }
        }
        out.push(format!("</{}>", element.name));
    }
    let document = letters_office::xml::parse(xml).unwrap();
    let mut out = Vec::new();
    walk(&document.root, &mut out);
    out
}

#[test]
#[ignore = "needs Microsoft Word (and converts pdfs, which takes a while)"]
fn a_docx_from_the_exact_converter_survives_an_untouched_save() {
    let before = office_processes();
    let corpus = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("exact")
        .join("tests")
        .join("corpus");
    let out = work_dir("exact");
    let mut converted = Vec::new();
    for name in ["syllabus", "slides", "form"] {
        let docx = out.join(format!("{name}.docx"));
        let options = letters_exact::ConvertOptions {
            use_word: false,
            verify: false,
            check_passes: false,
            ..Default::default()
        };
        timed(&format!("the exact converter makes {name}"), || {
            letters_exact::convert(&corpus.join(format!("{name}.pdf")), &docx, &options).unwrap()
        });
        converted.push(docx);
    }
    let resaved_dir = out.join("resaved");
    std::fs::create_dir_all(&resaved_dir).unwrap();
    let resaved: Vec<PathBuf> = converted.iter().map(|p| resave(p, &resaved_dir)).collect();
    let mut table = String::new();
    for (a, b) in converted.iter().zip(&resaved) {
        let first = canonical(&support::part_text(
            &std::fs::read(a).unwrap(),
            "word/document.xml",
        ));
        let second = canonical(&support::part_text(
            &std::fs::read(b).unwrap(),
            "word/document.xml",
        ));
        let first_difference = first.iter().zip(&second).position(|(x, y)| x != y);
        let _ = writeln!(
            table,
            "{}: document.xml {} ({} vs {} pieces{})",
            a.file_name().unwrap().to_string_lossy(),
            if first == second {
                "the same"
            } else {
                "differs"
            },
            first.len(),
            second.len(),
            first_difference
                .map(|at| format!(", first at {at}: {:?} vs {:?}", first[at], second[at]))
                .unwrap_or_default()
        );
    }
    let all: Vec<PathBuf> = converted.iter().chain(&resaved).cloned().collect();
    let pdfs = timed("word renders", || word_pdfs(&all, &out.join("pdf")));
    let found = differences(&pdfs[..converted.len()], &pdfs[converted.len()..]);
    let mut rows = String::from(TABLE_HEAD);
    for (path, difference) in converted.iter().zip(&found) {
        row(
            &mut rows,
            &path.file_name().unwrap().to_string_lossy(),
            "Word",
            difference.as_ref(),
        );
    }
    table.push('\n');
    table.push_str(&rows);
    eprintln!("{table}");
    write_results("exact_converter.md", &table);
    assert_none_left(&before);
    assert!(
        found
            .iter()
            .all(|d| d.as_ref().is_some_and(|d| d.identical(EXPORT_NOISE_PT))),
        "{table}"
    );
}

/// a page of one-line text boxes stacked exactly, as a converted pdf page
/// often is, for the box-merging part of tidy.
fn stacked_lines() -> PathBuf {
    use letters_office::model::*;
    let mut document = Document::new();
    document.blocks.clear();
    for line in 0..12 {
        let mut paragraph = Paragraph::with_text(&format!(
            "Line {line} of a converted page, one text box each."
        ));
        paragraph.props.spacing = Spacing {
            before: Some(0),
            after: Some(0),
            line: Some(LineSpacing {
                rule: LineRule::Exact,
                value: 300,
            }),
        };
        let mut text_box = TextBox::new(
            1440,
            2000 + 300 * line,
            7000,
            300,
            vec![Block::Paragraph(paragraph)],
        );
        text_box.border = None;
        text_box.fill = None;
        text_box.margins = BoxMargins {
            left: 0,
            top: 0,
            right: 0,
            bottom: 0,
        };
        let with = if line == 0 {
            AnchorWith::Own
        } else {
            AnchorWith::Previous
        };
        text_box.anchor = Some(ObjectAnchor::with(with));
        document.blocks.push(Block::TextBox(text_box));
    }
    let path = work_dir("tidy").join("stacked_lines.docx");
    std::fs::write(&path, write_docx(&document).unwrap()).unwrap();
    path
}

#[test]
#[ignore = "needs Microsoft Word and LibreOffice"]
fn tidy_never_changes_the_look() {
    use letters_office::tidy::{tidy, TidyOptions};
    let before = office_processes();
    let (word, libreoffice) = corpus();
    let out = work_dir("tidy");
    let mut files: Vec<(String, PathBuf)> = word.into_iter().chain(libreoffice).collect();
    files.push(("stacked_lines".into(), stacked_lines()));
    let options = TidyOptions {
        merge_stacked_text_boxes: true,
        ..TidyOptions::default()
    };
    let mut plain = Vec::new();
    let mut tidied = Vec::new();
    let mut sizes = String::from("| document | saved bytes | tidied bytes | saving | runs merged | marks dropped | styles removed | boxes merged |\n|---|---|---|---|---|---|---|---|\n");
    for (name, path) in &files {
        let bytes = std::fs::read(path).unwrap();
        let docx = path.extension().is_some_and(|e| e == "docx");
        let document = if docx {
            read_docx(&bytes).unwrap()
        } else {
            read_odt(&bytes).unwrap()
        };
        let (clean, report) = tidy(&document, &options);
        let write = |d: &letters_office::model::Document| {
            if docx {
                write_docx(d).unwrap()
            } else {
                write_odt(d).unwrap()
            }
        };
        let (saved, cleaned) = (write(&document), write(&clean));
        let extension = if docx { "docx" } else { "odt" };
        let plain_path = out.join(format!("{name}.saved.{extension}"));
        let tidy_path = out.join(format!("{name}.tidy.{extension}"));
        std::fs::write(&plain_path, &saved).unwrap();
        std::fs::write(&tidy_path, &cleaned).unwrap();
        let _ = writeln!(
            sizes,
            "| {name} | {} | {} | {:.1}% | {} | {} | {} | {} |",
            saved.len(),
            cleaned.len(),
            100.0 * (saved.len() as f64 - cleaned.len() as f64) / saved.len() as f64,
            report.runs_merged,
            report.marks_dropped,
            report.styles_removed,
            report.text_boxes_merged
        );
        plain.push(plain_path);
        tidied.push(tidy_path);
    }
    let all: Vec<PathBuf> = plain.iter().chain(&tidied).cloned().collect();
    let by_word = timed("word renders", || word_pdfs(&all, &out.join("word")));
    let by_lo = timed("libreoffice renders", || {
        libreoffice_pdfs(&all, &out.join("libreoffice"))
    });
    let n = plain.len();
    let word_differences = differences(&by_word[..n], &by_word[n..]);
    let lo_differences = differences(&by_lo[..n], &by_lo[n..]);
    let mut table = String::from(TABLE_HEAD);
    let mut moved = Vec::new();
    for (index, (name, _)) in files.iter().enumerate() {
        for (renderer, found) in [
            ("Word", &word_differences[index]),
            ("LibreOffice", &lo_differences[index]),
        ] {
            row(&mut table, name, renderer, found.as_ref());
            if !found.as_ref().is_some_and(|d| d.identical(EXPORT_NOISE_PT)) {
                moved.push(format!("{name} in {renderer}"));
            }
        }
    }
    let text = format!("{sizes}\n{table}");
    eprintln!("{text}");
    write_results("tidy.md", &text);
    assert_none_left(&before);
    assert!(moved.is_empty(), "tidy moved: {moved:?}");
}

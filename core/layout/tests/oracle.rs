//! the oracle harness: lays the corpus out with word, libreoffice and our
//! engine, and scores how close they are. every test here is ignored by
//! default because it needs the oracles' pdfs; `tests/oracle/run.ps1` runs
//! the whole loop (generate, author, export with both oracles, score).

#[path = "oracle/analysis.rs"]
pub mod analysis;
#[path = "oracle/corpus.rs"]
pub mod corpus;
#[path = "oracle/corpus2.rs"]
pub mod corpus2;
#[path = "oracle/lines.rs"]
pub mod lines;
#[path = "oracle/pdf_glyphs.rs"]
pub mod pdf_glyphs;
#[path = "oracle/probes.rs"]
pub mod probes;
#[path = "oracle/render.rs"]
pub mod render;
#[path = "oracle/report.rs"]
pub mod report;
#[path = "oracle/score.rs"]
pub mod score;

use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};

use letters_layout::{layout, Flavor, LayoutOptions};

pub fn out_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/oracle/out")
}

/// writes every corpus document as docx into `out/corpus`.
#[test]
#[ignore = "writes the oracle corpus; run by tests/oracle/run.ps1"]
fn generate_corpus() {
    let folder = out_dir().join("corpus");
    std::fs::create_dir_all(&folder).expect("corpus folder");
    for item in corpus::corpus() {
        let bytes = letters_office::write_docx(&item.document).expect("docx");
        std::fs::write(folder.join(format!("{}.docx", item.name)), bytes).expect("write");
    }
    for entry in corpus2::corpus2() {
        let bytes = letters_office::write_docx(&entry.document.document).expect("docx");
        let bytes = if entry.patches.is_empty() {
            bytes
        } else {
            patch_docx(&bytes, &entry.patches)
        };
        let path = folder.join(format!("{}.docx", entry.document.name));
        std::fs::write(path, bytes).expect("write");
    }
    // word-made documents from the fidelity work (read only): real files
    // with word's own styles, settings and theme
    let fidelity = Path::new("C:/Letters-fidelity/tools/fidelity/corpus");
    for name in ["syllabus", "research_paper", "lab_report"] {
        let source = fidelity.join(format!("{name}.docx"));
        if let Ok(bytes) = std::fs::read(&source) {
            std::fs::write(folder.join(format!("real_{name}.docx")), bytes).expect("write");
        }
    }
}

/// writes the probe documents into out/probes (exported by run.ps1 but
/// never scored with the corpus).
#[test]
#[ignore = "writes the probes; run by tests/oracle/run.ps1"]
fn generate_probes() {
    let folder = out_dir().join("probes");
    std::fs::create_dir_all(&folder).expect("probes folder");
    for probe in probes::probes() {
        let bytes = letters_office::write_docx(&probe.document).expect("docx");
        let bytes = patch_docx(&bytes, &probe.patches);
        std::fs::write(folder.join(format!("{}.docx", probe.name)), bytes).expect("write");
    }
}

/// for every table probe: the column widths each application chose, the
/// narrowest and natural widths our engine measures for each column, and
/// what candidate autofit rules give. this is how the autofit rule was
/// chosen (see tables.rs).
#[test]
#[ignore = "needs the probes' pdfs"]
fn fit_table_probes() {
    for probe in probes::probes() {
        let path = out_dir()
            .join("probes")
            .join(format!("{}.docx", probe.name));
        let Ok(bytes) = std::fs::read(&path) else {
            continue;
        };
        let document = letters_office::open(&bytes).expect("probe");
        let tables: Vec<usize> = document
            .blocks
            .iter()
            .enumerate()
            .filter(|(_, b)| matches!(b, letters_office::model::Block::Table(_)))
            .map(|(i, _)| i)
            .collect();
        for oracle in ["probes_word", "probes_lo"] {
            let Ok(pages) = pdf_glyphs::read_pdf(
                &out_dir()
                    .join(oracle)
                    .join(format!("{}_docx.pdf", probe.name)),
            ) else {
                continue;
            };
            println!("== {} {oracle}", probe.name);
            for (number, block) in tables.iter().enumerate() {
                let Some(page) = pages.get(number) else {
                    continue;
                };
                let mut edges = page.vertical_rules.clone();
                edges.sort_by(f64::total_cmp);
                edges.dedup_by(|a, b| (*a - *b).abs() < 0.33);
                let widths: Vec<f64> = edges.windows(2).map(|w| w[1] - w[0]).collect();
                let measured = letters_layout::tables::column_content_widths(
                    &document,
                    *block,
                    letters_layout::Flavor::Word,
                );
                // the first glyph of the table's first cell, from its left edge
                let inset = edges.first().and_then(|left| {
                    page.glyphs
                        .iter()
                        .filter(|g| g.x > *left && g.y > 90.0)
                        .map(|g| g.x - left)
                        .reduce(f64::min)
                });
                println!(
                    "  probe {number:2}: edges {:?} widths {:?} inset {:?}\n            ours (min, max): {:?}",
                    edges.iter().map(|e| (e * 100.0).round() / 100.0).collect::<Vec<_>>(),
                    widths.iter().map(|w| (w * 100.0).round() / 100.0).collect::<Vec<_>>(),
                    inset.map(|i| (i * 100.0).round() / 100.0),
                    measured
                        .iter()
                        .map(|(a, b)| ((a * 100.0).round() / 100.0, (b * 100.0).round() / 100.0))
                        .collect::<Vec<_>>()
                );
            }
        }
    }
}

/// for every justification probe paragraph: how far its first line with
/// the next word is over the measure, as a share of the line's spaces,
/// and whether each application squeezed that word in. prints, per probe
/// and application, the largest share squeezed and the smallest refused:
/// the squeeze rule lies between them.
#[test]
#[ignore = "needs the oracles' pdfs"]
fn fit_justify_probes() {
    for file in corpus_files()
        .into_iter()
        .filter(|f| f.name.starts_with("justify_probe"))
    {
        let document = letters_office::open(&std::fs::read(&file.path).unwrap()).unwrap();
        let shares = probe_shares(&document);
        for oracle in ["word", "lo"] {
            let Some(laid) = oracle_laid(oracle, &file.name, "docx") else {
                continue;
            };
            let (mut squeezed, mut refused) = (0.0_f64, f64::INFINITY);
            let mut next_line = 0;
            for (share, words, _) in &shares {
                // paragraphs come in order: look from the last match on, by
                // the first three words (random words repeat)
                let opening: String = words.iter().take(3).map(String::as_str).collect();
                let Some(found) = laid.lines[next_line..]
                    .iter()
                    .position(|l| l.text.starts_with(opening.as_str()))
                else {
                    continue;
                };
                let line = &laid.lines[next_line + found];
                next_line += found + 1;
                let joined: String = words.concat();
                if std::env::var("ORACLE_VERBOSE").is_ok() {
                    println!(
                        "    {share:.3} {} spaces {}",
                        if line.text.len() >= joined.len() {
                            "in "
                        } else {
                            "out"
                        },
                        words.len() - 1
                    );
                }
                if line.text.len() >= joined.len() {
                    squeezed = squeezed.max(*share);
                } else {
                    refused = refused.min(*share);
                }
            }
            println!(
                "{:28} {oracle:5}: squeezed up to {squeezed:.3}, refused from {refused:.3}",
                file.name
            );
        }
    }
}

/// each probe paragraph's overflow share, its first line's words with the
/// word that may join it, and its first word.
fn probe_shares(document: &letters_office::model::Document) -> Vec<(f64, Vec<String>, String)> {
    let family = document
        .styles
        .defaults
        .run
        .font
        .as_ref()
        .map(|f| f.name.clone())
        .unwrap_or_default();
    let size = f64::from(document.styles.defaults.run.size.unwrap_or(24)) / 2.0;
    let mut library = letters_layout::fonts::FontLibrary::system();
    let id = library.resolve(&family, false, false).expect("font");
    let face = library.font(id).face.clone();
    let width = |text: &str| -> f64 {
        text.chars()
            .map(|ch| f64::from(face.glyph(ch).map_or(0, |g| g.1)) * face.scale(size))
            .sum()
    };
    let space = width(" ");
    let mut out = Vec::new();
    for block in &document.blocks {
        let Some(paragraph) = block.paragraph() else {
            continue;
        };
        let text = paragraph.plain_text();
        let words: Vec<&str> = text.split(' ').collect();
        let mut line = 0.0;
        for (index, word) in words.iter().enumerate() {
            let with = if index == 0 {
                width(word)
            } else {
                line + space + width(word)
            };
            if with > 468.0 && index > 0 {
                let share = (with - 468.0) / (index as f64 * space);
                let first: Vec<String> = words[..=index].iter().map(|w| w.to_string()).collect();
                out.push((share, first, words[0].to_string()));
                break;
            }
            line = with;
        }
    }
    out
}

/// rewrites word/document.xml of a package with the patches.
fn patch_docx(bytes: &[u8], patches: &[corpus2::Patch]) -> Vec<u8> {
    let mut archive = zip::ZipArchive::new(std::io::Cursor::new(bytes)).expect("our docx");
    let mut out = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    let options = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);
    for index in 0..archive.len() {
        let mut file = archive.by_index(index).expect("entry");
        let name = file.name().map(|n| n.to_string()).expect("entry name");
        let mut content = Vec::new();
        file.read_to_end(&mut content).expect("read entry");
        if name == "word/document.xml" {
            let text = String::from_utf8(content).expect("utf-8 xml");
            content = corpus2::apply_patches(&text, patches).into_bytes();
        }
        out.start_file(name, options).expect("start");
        out.write_all(&content).expect("write entry");
    }
    out.finish().expect("finish").into_inner()
}

/// one document every application lays out.
#[derive(Clone, Debug)]
pub struct CorpusFile {
    pub name: String,
    pub class: String,
    pub held_out: bool,
    pub path: PathBuf,
    /// "docx" or "odt"
    pub format: &'static str,
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct ManifestEntry {
    name: String,
    class: String,
    held_out: bool,
    status: String,
}

/// every document of the corpus: ours (out/corpus), the ones word and
/// libreoffice made themselves (out/authored_word, out/authored_lo).
pub fn corpus_files() -> Vec<CorpusFile> {
    let first: Vec<(String, &'static str)> = corpus::corpus()
        .into_iter()
        .map(|item| (item.name, item.class))
        .collect();
    let second: Vec<(String, &'static str, bool)> = corpus2::corpus2()
        .into_iter()
        .map(|entry| (entry.document.name, entry.document.class, entry.held_out))
        .collect();
    let mut files = Vec::new();
    let folder = out_dir().join("corpus");
    let mut names: Vec<String> = std::fs::read_dir(&folder)
        .into_iter()
        .flatten()
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "docx"))
        .map(|path| path.file_stem().unwrap().to_string_lossy().to_string())
        .collect();
    names.sort();
    for name in names {
        let (class, held_out) = if name.starts_with("real_") {
            ("word-made documents".to_string(), false)
        } else if let Some((_, class)) = first.iter().find(|(n, _)| *n == name) {
            (class.to_string(), false)
        } else if let Some((_, class, held)) = second.iter().find(|(n, _, _)| *n == name) {
            (class.to_string(), *held)
        } else {
            continue;
        };
        files.push(CorpusFile {
            path: folder.join(format!("{name}.docx")),
            name,
            class,
            held_out,
            format: "docx",
        });
    }
    for (folder, format) in [("authored_word", "docx"), ("authored_lo", "odt")] {
        let folder = out_dir().join(folder);
        let Ok(text) = std::fs::read_to_string(folder.join("manifest.json")) else {
            continue;
        };
        let entries: Vec<ManifestEntry> = serde_json::from_str(&text).unwrap_or_default();
        for entry in entries.into_iter().filter(|e| e.status == "ok") {
            files.push(CorpusFile {
                path: folder.join(format!("{}.{format}", entry.name)),
                name: entry.name,
                class: entry.class,
                held_out: entry.held_out,
                format,
            });
        }
    }
    files
}

fn oracle_laid(folder: &str, stem: &str, suffix: &str) -> Option<score::Laid> {
    let path = out_dir().join(folder).join(format!("{stem}_{suffix}.pdf"));
    let pages = pdf_glyphs::read_pdf(&path).ok()?;
    Some(score::Laid::of_pdf(&pages))
}

fn oracle_lines(folder: &str, stem: &str, suffix: &str) -> Option<(usize, Vec<lines::TextLine>)> {
    oracle_laid(folder, stem, suffix).map(|laid| (laid.pages, laid.lines))
}

fn body_zone(document: &letters_office::model::Document) -> score::BodyZone {
    let section = document.sections.first().cloned().unwrap_or_default();
    let geometry = letters_layout::pages::geometry::SectionGeometry::of(&section);
    score::BodyZone {
        top: geometry.top - 4.0,
        bottom: geometry.height - geometry.bottom + 4.0,
    }
}

fn compare(
    reference: &Option<score::Laid>,
    candidate: &Option<score::Laid>,
    zone: score::BodyZone,
) -> Option<score::Score> {
    Some(score::score(reference.as_ref()?, candidate.as_ref()?, zone))
}

/// the open fonts that come with letters, and nothing else: a computer
/// without microsoft's fonts (vendor/fonts; tools/fetch-fonts.ps1 fills it).
pub fn open_fonts() -> Arc<letters_layout::fonts::LibraryFonts> {
    static FONTS: OnceLock<Arc<letters_layout::fonts::LibraryFonts>> = OnceLock::new();
    FONTS
        .get_or_init(|| {
            let folder = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../vendor/fonts");
            Arc::new(letters_layout::fonts::LibraryFonts::bundled_only(&folder))
        })
        .clone()
}

fn open_fonts_options(flavor: Flavor) -> LayoutOptions {
    LayoutOptions {
        flavor,
        fonts: open_fonts(),
        ..LayoutOptions::default()
    }
}

/// lays a document out, writes our pdf and reads it back like an oracle's.
fn ours(
    document: &letters_office::model::Document,
    options: &LayoutOptions,
    pdf_name: &str,
) -> (
    Option<score::Laid>,
    Option<PathBuf>,
    letters_layout::LayoutResult,
) {
    let result = layout(document, options);
    let pdf = write_our_pdf(pdf_name, &result, document);
    let laid = pdf
        .as_ref()
        .and_then(|path| pdf_glyphs::read_pdf(path).ok())
        .map(|pages| score::Laid::of_pdf(&pages))
        .or_else(|| Some(score::lines_of_result(&result)));
    (laid, pdf, result)
}

/// lays every corpus document out with both flavors, scores them against
/// both oracles and against each other, and writes ORACLE_REPORT.md.
#[test]
#[ignore = "needs the oracles' pdfs; run by tests/oracle/run.ps1"]
fn score_corpus() {
    let only = std::env::var("ORACLE_ONLY").ok();
    let mut documents = Vec::new();
    for file in corpus_files() {
        if only
            .as_ref()
            .is_some_and(|only| !file.name.contains(only.as_str()))
        {
            continue;
        }
        let Ok(bytes) = std::fs::read(&file.path) else {
            continue;
        };
        let Ok(document) = letters_office::open(&bytes) else {
            println!("{}: could not be read", file.name);
            continue;
        };
        let name = &file.name;
        let zone = body_zone(&document);
        let word = oracle_laid("word", name, file.format);
        let libre = oracle_laid("lo", name, file.format);
        let started = std::time::Instant::now();
        let _ = layout(&document, &LayoutOptions::flavor(Flavor::Word));
        let _ = layout(&document, &LayoutOptions::flavor(Flavor::LibreOffice));
        let layout_ms = started.elapsed().as_secs_f64() * 1000.0;
        let (ours_word, pdf_word, result_word) = ours(
            &document,
            &LayoutOptions::flavor(Flavor::Word),
            &format!("{name}_word"),
        );
        save_layout(name, &result_word);
        let (ours_libre, pdf_libre, _) = ours(
            &document,
            &LayoutOptions::flavor(Flavor::LibreOffice),
            &format!("{name}_lo"),
        );
        let (open_word, _, _) = ours(
            &document,
            &open_fonts_options(Flavor::Word),
            &format!("{name}_word_open"),
        );
        let (open_libre, _, _) = ours(
            &document,
            &open_fonts_options(Flavor::LibreOffice),
            &format!("{name}_lo_open"),
        );
        let mut scores = report::DocumentScores {
            name: name.clone(),
            class: file.class.clone(),
            held_out: file.held_out,
            format: file.format.to_string(),
            libreoffice_vs_word: compare(&word, &libre, zone),
            letters_vs_word: compare(&word, &ours_word, zone),
            word_vs_libreoffice: compare(&libre, &word, zone),
            letters_vs_libreoffice: compare(&libre, &ours_libre, zone),
            letters_vs_word_open_fonts: compare(&word, &open_word, zone),
            letters_vs_libreoffice_open_fonts: compare(&libre, &open_libre, zone),
            layout_ms,
            ..Default::default()
        };
        if file.format == "docx" {
            score_odt(&mut scores, name);
        }
        if std::env::var("ORACLE_RENDER").is_ok() {
            add_render_scores(
                &mut scores,
                name,
                file.format,
                pdf_word.as_deref(),
                pdf_libre.as_deref(),
            );
        }
        for (label, score) in [
            ("vs word", &scores.letters_vs_word),
            ("vs lo  ", &scores.letters_vs_libreoffice),
        ] {
            if let Some(s) = score {
                println!(
                    "{name:30} {label}: pages {}/{} same lines {:5.1}% breaks {:5.1}% baseline {:.2}/{:.2}",
                    s.reference_pages,
                    s.candidate_pages,
                    s.same_line_share() * 100.0,
                    s.page_break_share() * 100.0,
                    s.baseline.mean,
                    s.baseline.max
                );
            }
        }
        documents.push(scores);
    }
    if only.is_some() {
        return;
    }
    let iteration = std::env::var("ORACLE_ITERATION").unwrap_or_else(|_| "unnamed".into());
    let history_path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/oracle/history.json");
    let mut history: Vec<report::HistoryEntry> = std::fs::read(&history_path)
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default();
    history.retain(|entry| entry.iteration != iteration);
    history.push(report::history_entry(&iteration, &documents));
    std::fs::write(&history_path, serde_json::to_vec_pretty(&history).unwrap()).expect("history");
    std::fs::write(
        out_dir().join("scores.json"),
        serde_json::to_vec(&documents).unwrap(),
    )
    .expect("scores");
    let fit = documents.iter().filter(|d| !d.held_out).count();
    let header = format!(
        "Generated by `cargo test -p letters-layout --release --test oracle -- --ignored score_corpus` (iteration `{iteration}`). Oracles: Microsoft Word 16.0 build 16.0.20527 and LibreOffice 26.8.2 on Windows 11, exporting the same files to PDF; glyph positions, rules and pictures read from every PDF (ours too) with pdfium. {} documents: {fit} in the fit set, {} held out (never used while fitting).",
        documents.len(),
        documents.len() - fit
    );
    let text = report::write_report(&documents, &history, &header, analysis::ANALYSIS);
    std::fs::write(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("ORACLE_REPORT.md"),
        text,
    )
    .expect("report");
}

/// the odt libreoffice made from a corpus document: word's and our
/// layouts of it scored against libreoffice's own.
fn score_odt(scores: &mut report::DocumentScores, name: &str) {
    let Ok(bytes) = std::fs::read(out_dir().join("odt").join(format!("{name}.odt"))) else {
        return;
    };
    let Ok(document) = letters_office::open(&bytes) else {
        return;
    };
    let zone = body_zone(&document);
    let libre = oracle_laid("lo_odt", name, "odt");
    let word = oracle_laid("word_odt", name, "odt");
    let (ours, _, _) = ours(
        &document,
        &LayoutOptions::flavor(Flavor::LibreOffice),
        &format!("{name}_odt_lo"),
    );
    scores.odt_word_vs_libreoffice = compare(&libre, &word, zone);
    scores.odt_letters_vs_libreoffice = compare(&libre, &ours, zone);
}

/// writes our pdf of a layout into out/letters; `None` if it failed.
fn write_our_pdf(
    name: &str,
    result: &letters_layout::LayoutResult,
    document: &letters_office::model::Document,
) -> Option<PathBuf> {
    let folder = out_dir().join("letters");
    std::fs::create_dir_all(&folder).ok()?;
    let bytes = letters_layout::pdf::write_pdf(result, document).ok()?;
    let path = folder.join(format!("{name}.pdf"));
    std::fs::write(&path, bytes).ok()?;
    Some(path)
}

/// renders the oracles' pdfs and ours at 150 dpi and compares them.
fn add_render_scores(
    scores: &mut report::DocumentScores,
    name: &str,
    format: &str,
    ours_word: Option<&Path>,
    ours_libre: Option<&Path>,
) {
    let Some(mut engine) = render::engine() else {
        return;
    };
    let word = render::render(
        &mut engine,
        &out_dir().join("word").join(format!("{name}_{format}.pdf")),
    )
    .ok();
    let libre = render::render(
        &mut engine,
        &out_dir().join("lo").join(format!("{name}_{format}.pdf")),
    )
    .ok();
    let ours_word = ours_word.and_then(|path| render::render(&mut engine, path).ok());
    let ours_libre = ours_libre.and_then(|path| render::render(&mut engine, path).ok());
    if let (Some(word), Some(ours)) = (&word, &ours_word) {
        scores.render_vs_word = render::compare_documents(word, ours);
    }
    if let (Some(libre), Some(ours)) = (&libre, &ours_libre) {
        scores.render_vs_libreoffice = render::compare_documents(libre, ours);
    }
    if let (Some(word), Some(libre)) = (&word, &libre) {
        scores.render_libreoffice_vs_word = render::compare_documents(word, libre);
        scores.render_word_vs_libreoffice = render::compare_documents(libre, word);
    }
}

/// keeps our layout as json next to the oracles' pdfs, for diagnosis.
fn save_layout(name: &str, result: &letters_layout::LayoutResult) {
    let folder = out_dir().join("letters");
    let _ = std::fs::create_dir_all(&folder);
    let _ = std::fs::write(
        folder.join(format!("{name}.json")),
        serde_json::to_vec(result).unwrap_or_default(),
    );
}

/// opens a corpus document by name, wherever it lives.
fn open_named(name: &str) -> (letters_office::model::Document, CorpusFile) {
    let file = corpus_files()
        .into_iter()
        .find(|file| file.name == name)
        .expect("a corpus document of that name");
    let bytes = std::fs::read(&file.path).expect("read");
    (letters_office::open(&bytes).expect("open"), file)
}

/// prints line pitch and the first baseline of each paragraph of the font
/// probes, as each oracle laid them out (the metrics measurement).
#[test]
#[ignore = "needs the oracles' pdfs"]
fn print_font_probe_lines() {
    for oracle in ["word", "lo"] {
        for entry in std::fs::read_dir(out_dir().join(oracle))
            .into_iter()
            .flatten()
            .flatten()
        {
            let path = entry.path();
            let name = path.file_name().unwrap().to_string_lossy().to_string();
            if !(name.starts_with("font_") || name.starts_with("spacing_modes"))
                || !name.ends_with(".pdf")
            {
                continue;
            }
            let pages = pdf_glyphs::read_pdf(&path).expect("pdf");
            let lines = lines::lines_of_document(&pages);
            println!("== {oracle} {name}");
            let mut previous: Option<&lines::TextLine> = None;
            for line in &lines {
                let pitch = previous
                    .filter(|p| p.page == line.page)
                    .map(|p| line.baseline - p.baseline)
                    .unwrap_or(0.0);
                println!(
                    "  p{} y {:8.3} x {:7.3} size {:5.2} pitch {:7.3} {}",
                    line.page,
                    line.baseline,
                    line.x,
                    line.size,
                    pitch,
                    line.text.chars().take(24).collect::<String>()
                );
                previous = Some(line);
            }
        }
    }
}

/// the font metrics measurement: for each font probe paragraph, the
/// measured mean line pitch and first-baseline offset next to what each
/// metric choice predicts. this is how the line height rule was chosen.
#[test]
#[ignore = "needs the oracles' pdfs"]
fn fit_font_metrics() {
    use letters_layout::fonts::{metrics::external_leading, FontLibrary};
    let mut library = FontLibrary::system();
    for oracle in ["word", "lo"] {
        println!("== {oracle}: family size | pitch measured | win  win+ext  hhea  typo | first baseline - top | win asc  asc+ext");
        for family in [
            "Calibri",
            "Cambria",
            "Times New Roman",
            "Arial",
            "Aptos",
            "Georgia",
            "Courier New",
        ] {
            let name = format!("font_{}_docx.pdf", family.to_lowercase().replace(' ', "_"));
            let Ok(pages) = pdf_glyphs::read_pdf(&out_dir().join(oracle).join(&name)) else {
                continue;
            };
            let lines = lines::lines_of_document(&pages);
            let Some(id) = library.resolve(family, false, false) else {
                continue;
            };
            let m = library.font(id).face.metrics;
            let upem = m.units_per_em;
            let mut top = 72.0;
            for size in [9.0, 11.0, 12.0, 16.0] {
                let group: Vec<&lines::TextLine> = lines
                    .iter()
                    .filter(|l| (l.size - size).abs() < 0.1)
                    .collect();
                if group.len() < 2 {
                    continue;
                }
                let pitch =
                    (group.last().unwrap().baseline - group[0].baseline) / (group.len() - 1) as f64;
                let win = (m.win_ascent + m.win_descent) / upem * size;
                let ext = external_leading(&m) / upem * size;
                let hhea = (m.hhea_ascent - m.hhea_descent + m.hhea_line_gap) / upem * size;
                let typo = (m.typo_ascent - m.typo_descent + m.typo_line_gap) / upem * size;
                let first = group[0].baseline - top;
                println!(
                    "  {family:16} {size:4} | {pitch:7.3} | {win:7.3} {:7.3} {hhea:7.3} {typo:7.3} | {first:7.3} | {:7.3} {:7.3} typo-flag {}",
                    win + ext,
                    m.win_ascent / upem * size,
                    (m.win_ascent / upem * size) + ext,
                    m.use_typo_metrics
                );
                // the next paragraph starts after this one's lines and 12 pt after
                top += group.len() as f64 * pitch + 12.0;
            }
        }
    }
}

#[test]
#[ignore = "diagnostic"]
fn print_face_metrics() {
    let mut library = letters_layout::fonts::FontLibrary::system();
    for family in ["Aptos", "Calibri", "Times New Roman", "Cambria", "Arial"] {
        let id = library.resolve(family, false, false).unwrap();
        println!(
            "{family}: {:?} {:?}",
            library.font(id).face.metrics,
            library.font(id).face.origin
        );
    }
    for oracle in ["word", "lo"] {
        let pages =
            pdf_glyphs::read_pdf(&out_dir().join(oracle).join("font_aptos_docx.pdf")).unwrap();
        println!("{oracle} aptos pdf font: {}", pages[0].glyphs[0].font);
    }
}

/// prints our lines next to the oracle's for one document
/// (ORACLE_ONLY=name, ORACLE_AGAINST=word|lo, ORACLE_FLAVOR=word|lo).
#[test]
#[ignore = "diagnostic"]
fn diagnose() {
    let name = std::env::var("ORACLE_ONLY").expect("ORACLE_ONLY=document name");
    let against = std::env::var("ORACLE_AGAINST").unwrap_or_else(|_| "word".into());
    let flavor = match std::env::var("ORACLE_FLAVOR").as_deref() {
        Ok("lo") => Flavor::LibreOffice,
        _ => Flavor::Word,
    };
    let (document, file) = open_named(&name);
    let result = layout(&document, &LayoutOptions::flavor(flavor));
    let ours = score::lines_of_result(&result).lines;
    let (_, theirs) = oracle_lines(&against, &name, file.format).expect("oracle pdf");
    let mut used = vec![false; ours.len()];
    for line in &theirs {
        let found = ours
            .iter()
            .enumerate()
            .filter(|(i, o)| !used[*i] && o.text == line.text)
            .min_by(|a, b| {
                ((a.1.page as f64 - line.page as f64).abs() * 1e4
                    + (a.1.baseline - line.baseline).abs())
                .total_cmp(
                    &((b.1.page as f64 - line.page as f64).abs() * 1e4
                        + (b.1.baseline - line.baseline).abs()),
                )
            });
        let text: String = line.text.chars().take(30).collect();
        match found {
            Some((index, other)) => {
                used[index] = true;
                println!(
                    "p{} y {:8.2} x {:7.2} | ours p{} dy {:+7.2} dx {:+6.2} | {text}",
                    line.page,
                    line.baseline,
                    line.x,
                    other.page,
                    other.baseline - line.baseline,
                    other.x - line.x
                );
            }
            None => {
                let near = ours.iter().filter(|o| o.page == line.page).min_by(|a, b| {
                    (a.baseline - line.baseline)
                        .abs()
                        .total_cmp(&(b.baseline - line.baseline).abs())
                });
                let ours_text: String = near
                    .map(|o| o.text.chars().take(30).collect())
                    .unwrap_or_default();
                println!(
                    "p{} y {:8.2} x {:7.2} | NO MATCH            | {text}\n{:>41}ours: {ours_text}",
                    line.page, line.baseline, line.x, ""
                );
            }
        }
    }
    println!("unsupported: {:?}", result.unsupported);
    println!("substitutions: {:?}", result.substitutions);
}

/// prints our lines of one paragraph with their natural widths and the
/// width of the word that did not fit (ORACLE_ONLY=name, ORACLE_BLOCK=index).
#[test]
#[ignore = "diagnostic"]
fn diagnose_paragraph() {
    let name = std::env::var("ORACLE_ONLY").expect("ORACLE_ONLY");
    let block: u32 = std::env::var("ORACLE_BLOCK")
        .ok()
        .and_then(|b| b.parse().ok())
        .unwrap_or(0);
    let (document, _) = open_named(&name);
    let result = layout(&document, &LayoutOptions::flavor(Flavor::Word));
    for page in &result.pages {
        for line in page.lines.iter().filter(|l| l.block == vec![block]) {
            let text: String = line.runs.iter().map(|r| r.text.as_str()).collect();
            let last = line
                .runs
                .last()
                .map(|r| (r.x.last().copied().unwrap_or(0.0), r.size))
                .unwrap_or_default();
            println!(
                "x {:7.2} w {:7.2} end {:7.2} last glyph x {:7.2} | {text}",
                line.x,
                line.width,
                line.x + line.width,
                last.0
            );
        }
    }
}

/// widths of a text in one font under different rounding hypotheses
/// (ORACLE_TEXT, ORACLE_FONT, ORACLE_SIZE): exact, per-glyph rounded to
/// 1/600 in, per-glyph floored, rounded at 1440 dpi.
#[test]
#[ignore = "diagnostic"]
fn measure_text() {
    let text = std::env::var("ORACLE_TEXT").expect("ORACLE_TEXT");
    let family = std::env::var("ORACLE_FONT").unwrap_or_else(|_| "Times New Roman".into());
    let size: f64 = std::env::var("ORACLE_SIZE")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(12.0);
    let mut library = letters_layout::fonts::FontLibrary::system();
    let id = library.resolve(&family, false, false).unwrap();
    let face = &library.font(id).face;
    let scale = face.scale(size);
    let (mut exact, mut round600, mut floor600, mut twips, mut ppem) = (0.0, 0.0, 0.0, 0.0, 0.0);
    // whole-pixel advances at the em size in 600 dpi pixels, as gdi gives them
    let em_px = (size / 0.12).round();
    for ch in text.chars() {
        let (_, advance) = face.glyph(ch).unwrap_or((0, 0));
        let points = f64::from(advance) * scale;
        exact += points;
        round600 += (points / 0.12).round() * 0.12;
        floor600 += (points / 0.12).floor() * 0.12;
        twips += (points * 20.0).round() / 20.0;
        ppem += (f64::from(advance) * em_px / face.metrics.units_per_em).round() * 0.12;
    }
    println!("exact {exact:.3} round600 {round600:.3} floor600 {floor600:.3} twips {twips:.3} ppem-rounded {ppem:.3}");
}

/// prints every line of one of our pages (ORACLE_ONLY, ORACLE_PAGE).
#[test]
#[ignore = "diagnostic"]
fn print_our_page() {
    let name = std::env::var("ORACLE_ONLY").expect("ORACLE_ONLY");
    let page: usize = std::env::var("ORACLE_PAGE")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(0);
    let (document, _) = open_named(&name);
    let result = layout(&document, &LayoutOptions::flavor(Flavor::Word));
    for line in &result.pages[page].lines {
        let text: String = line.runs.iter().map(|r| r.text.as_str()).collect();
        println!(
            "{:?} {:?} top {:7.2} base {:7.2} h {:6.2} | {}",
            line.story,
            line.block,
            line.top,
            line.baseline,
            line.height,
            text.chars().take(50).collect::<String>()
        );
    }
}

#[test]
#[ignore = "diagnostic"]
fn print_carry_parts() {
    let name = std::env::var("ORACLE_ONLY").expect("ORACLE_ONLY");
    let (document, _) = open_named(&name);
    for part in &document.carry.parts {
        println!("part {} {} bytes", part.name, part.bytes.len());
    }
    for rels in &document.carry.part_relationships {
        println!(
            "rels of {}: {:?}",
            rels.part,
            rels.relationships
                .iter()
                .map(|r| (&r.id, &r.target))
                .collect::<Vec<_>>()
        );
    }
    println!(
        "embedded: {:?}",
        letters_layout::fonts::embedded::embedded_faces(&document)
            .iter()
            .map(|f| (&f.family, f.bold, f.italic, f.bytes.len()))
            .collect::<Vec<_>>()
    );
}

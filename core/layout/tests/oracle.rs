//! the oracle harness: lays the corpus out with word, libreoffice and our
//! engine, and scores how close they are. every test here is ignored by
//! default because it needs the oracles' pdfs; `tests/oracle/run.ps1` runs
//! the whole loop (generate, export with both oracles, score).

#[path = "oracle/analysis.rs"]
pub mod analysis;
#[path = "oracle/corpus.rs"]
pub mod corpus;
#[path = "oracle/lines.rs"]
pub mod lines;
#[path = "oracle/pdf_glyphs.rs"]
pub mod pdf_glyphs;
#[path = "oracle/render.rs"]
pub mod render;
#[path = "oracle/report.rs"]
pub mod report;
#[path = "oracle/score.rs"]
pub mod score;

use std::path::{Path, PathBuf};

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

/// the class each corpus document is scored in.
fn class_of(name: &str) -> String {
    if name.starts_with("real_") {
        return "word-made documents".into();
    }
    corpus::corpus()
        .into_iter()
        .find(|item| item.name == name)
        .map(|item| item.class.to_string())
        .unwrap_or_else(|| "other".into())
}

fn oracle_lines(folder: &str, stem: &str, suffix: &str) -> Option<(usize, Vec<lines::TextLine>)> {
    let path = out_dir().join(folder).join(format!("{stem}_{suffix}.pdf"));
    let pages = pdf_glyphs::read_pdf(&path).ok()?;
    Some((pages.len(), lines::lines_of_document(&pages)))
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
    reference: &Option<(usize, Vec<lines::TextLine>)>,
    candidate: &Option<(usize, Vec<lines::TextLine>)>,
    zone: score::BodyZone,
) -> Option<score::Score> {
    let (reference, candidate) = (reference.as_ref()?, candidate.as_ref()?);
    Some(score::score(
        (reference.0, &reference.1),
        (candidate.0, &candidate.1),
        zone,
    ))
}

/// lays every corpus document out with both flavors, scores them against
/// both oracles and against each other, and writes ORACLE_REPORT.md.
#[test]
#[ignore = "needs the oracles' pdfs; run by tests/oracle/run.ps1"]
fn score_corpus() {
    use letters_layout::{layout, Flavor, LayoutOptions};
    let corpus_folder = out_dir().join("corpus");
    let mut names: Vec<String> = std::fs::read_dir(&corpus_folder)
        .expect("generate the corpus first")
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "docx"))
        .map(|path| path.file_stem().unwrap().to_string_lossy().to_string())
        .collect();
    names.sort();
    let only = std::env::var("ORACLE_ONLY").ok();
    let mut documents = Vec::new();
    for name in names {
        if only
            .as_ref()
            .is_some_and(|only| !name.contains(only.as_str()))
        {
            continue;
        }
        let bytes = std::fs::read(corpus_folder.join(format!("{name}.docx"))).expect("read");
        let Ok(document) = letters_office::open(&bytes) else {
            println!("{name}: could not be read");
            continue;
        };
        let zone = body_zone(&document);
        let word = oracle_lines("word", &name, "docx");
        let libre = oracle_lines("lo", &name, "docx");
        let started = std::time::Instant::now();
        let ours_word = layout(&document, &LayoutOptions::flavor(Flavor::Word));
        let ours_libre = layout(&document, &LayoutOptions::flavor(Flavor::LibreOffice));
        let layout_ms = started.elapsed().as_secs_f64() * 1000.0;
        save_layout(&name, &ours_word);
        // our own pdf, read back by the same reader as the oracles' pdfs
        let our_pdf_word = write_our_pdf(&name, "word", &ours_word, &document);
        let our_pdf_libre = write_our_pdf(&name, "lo", &ours_libre, &document);
        let ours_word = our_pdf_word
            .as_ref()
            .and_then(|path| pdf_glyphs::read_pdf(path).ok())
            .map(|pages| (pages.len(), lines::lines_of_document(&pages)))
            .or_else(|| Some(score::lines_of_result(&ours_word)));
        let ours_libre = our_pdf_libre
            .as_ref()
            .and_then(|path| pdf_glyphs::read_pdf(path).ok())
            .map(|pages| (pages.len(), lines::lines_of_document(&pages)))
            .or_else(|| Some(score::lines_of_result(&ours_libre)));
        let scores = report::DocumentScores {
            name: name.clone(),
            class: class_of(&name),
            libreoffice_vs_word: compare(&word, &libre, zone),
            letters_vs_word: compare(&word, &ours_word, zone),
            word_vs_libreoffice: compare(&libre, &word, zone),
            letters_vs_libreoffice: compare(&libre, &ours_libre, zone),
            layout_ms,
            ..Default::default()
        };
        let mut scores = scores;
        score_odt(&mut scores, &name);
        if std::env::var("ORACLE_RENDER").is_ok() {
            add_render_scores(
                &mut scores,
                &name,
                our_pdf_word.as_deref(),
                our_pdf_libre.as_deref(),
            );
        }
        if let Some(s) = &scores.letters_vs_word {
            println!(
                "{name:28} vs word: pages {}/{} same lines {:5.1}% breaks {:5.1}% baseline {:.2}/{:.2}",
                s.reference_pages,
                s.candidate_pages,
                s.same_line_share() * 100.0,
                s.page_break_share() * 100.0,
                s.baseline.mean,
                s.baseline.max
            );
        }
        if let Some(s) = &scores.letters_vs_libreoffice {
            println!(
                "{name:28} vs lo:   pages {}/{} same lines {:5.1}% breaks {:5.1}% baseline {:.2}/{:.2}",
                s.reference_pages,
                s.candidate_pages,
                s.same_line_share() * 100.0,
                s.page_break_share() * 100.0,
                s.baseline.mean,
                s.baseline.max
            );
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
        serde_json::to_vec_pretty(&documents).unwrap(),
    )
    .expect("scores");
    let analysis = analysis::ANALYSIS;
    let header = format!(
        "Generated by `cargo test -p letters-layout --release --test oracle -- --ignored score_corpus` (iteration `{iteration}`). Oracles: Microsoft Word 16.0 build 16.0.20527 and LibreOffice 26.8.2 on Windows 11, exporting the same files to PDF; glyph positions read from every PDF (ours too) with pdfium. {} documents.",
        documents.len()
    );
    let text = report::write_report(&documents, &history, &header, analysis);
    std::fs::write(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("ORACLE_REPORT.md"),
        text,
    )
    .expect("report");
}

/// the odt libreoffice made from a corpus document: word's and our
/// layouts of it scored against libreoffice's own.
fn score_odt(scores: &mut report::DocumentScores, name: &str) {
    use letters_layout::{layout, Flavor, LayoutOptions};
    let Ok(bytes) = std::fs::read(out_dir().join("odt").join(format!("{name}.odt"))) else {
        return;
    };
    let Ok(document) = letters_office::open(&bytes) else {
        return;
    };
    let zone = body_zone(&document);
    let libre = oracle_lines("lo_odt", name, "odt");
    let word = oracle_lines("word_odt", name, "odt");
    let ours = layout(&document, &LayoutOptions::flavor(Flavor::LibreOffice));
    let ours = write_our_pdf(&format!("{name}_odt"), "lo", &ours, &document)
        .and_then(|path| pdf_glyphs::read_pdf(&path).ok())
        .map(|pages| (pages.len(), lines::lines_of_document(&pages)));
    scores.odt_word_vs_libreoffice = compare(&libre, &word, zone);
    scores.odt_letters_vs_libreoffice = compare(&libre, &ours, zone);
}

/// writes our pdf of a layout into out/letters; `None` if it failed.
fn write_our_pdf(
    name: &str,
    flavor: &str,
    result: &letters_layout::LayoutResult,
    document: &letters_office::model::Document,
) -> Option<PathBuf> {
    let folder = out_dir().join("letters");
    std::fs::create_dir_all(&folder).ok()?;
    let bytes = letters_layout::pdf::write_pdf(result, document).ok()?;
    let path = folder.join(format!("{name}_{flavor}.pdf"));
    std::fs::write(&path, bytes).ok()?;
    Some(path)
}

/// renders the oracles' pdfs and ours at 300 dpi and compares them.
fn add_render_scores(
    scores: &mut report::DocumentScores,
    name: &str,
    ours_word: Option<&Path>,
    ours_libre: Option<&Path>,
) {
    let Some(mut engine) = render::engine() else {
        return;
    };
    let word = render::render(
        &mut engine,
        &out_dir().join("word").join(format!("{name}_docx.pdf")),
    )
    .ok();
    let libre = render::render(
        &mut engine,
        &out_dir().join("lo").join(format!("{name}_docx.pdf")),
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
    use letters_layout::{layout, Flavor, LayoutOptions};
    let name = std::env::var("ORACLE_ONLY").expect("ORACLE_ONLY=document name");
    let against = std::env::var("ORACLE_AGAINST").unwrap_or_else(|_| "word".into());
    let flavor = match std::env::var("ORACLE_FLAVOR").as_deref() {
        Ok("lo") => Flavor::LibreOffice,
        _ => Flavor::Word,
    };
    let bytes = std::fs::read(out_dir().join("corpus").join(format!("{name}.docx"))).expect("read");
    let document = letters_office::open(&bytes).expect("open");
    let result = layout(&document, &LayoutOptions::flavor(flavor));
    let (_, ours) = score::lines_of_result(&result);
    let (_, theirs) = oracle_lines(&against, &name, "docx").expect("oracle pdf");
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
    use letters_layout::{layout, Flavor, LayoutOptions};
    let name = std::env::var("ORACLE_ONLY").expect("ORACLE_ONLY");
    let block: u32 = std::env::var("ORACLE_BLOCK")
        .ok()
        .and_then(|b| b.parse().ok())
        .unwrap_or(0);
    let bytes = std::fs::read(out_dir().join("corpus").join(format!("{name}.docx"))).expect("read");
    let document = letters_office::open(&bytes).expect("open");
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

/// for every line of an oracle's pdf (single-font documents), its natural
/// width by our metrics minus the room it had (ORACLE_ONLY, ORACLE_FONT,
/// ORACLE_SIZE, ORACLE_AGAINST). positive values are lines the oracle
/// fitted although they are wider than the room.
#[test]
#[ignore = "diagnostic"]
fn oracle_line_overflow() {
    let name = std::env::var("ORACLE_ONLY").expect("ORACLE_ONLY");
    let against = std::env::var("ORACLE_AGAINST").unwrap_or_else(|_| "word".into());
    let family = std::env::var("ORACLE_FONT").unwrap_or_else(|_| "Times New Roman".into());
    let size: f64 = std::env::var("ORACLE_SIZE")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(12.0);
    let mut library = letters_layout::fonts::FontLibrary::system();
    let id = library.resolve(&family, false, false).unwrap();
    let face = library.font(id).face.clone();
    let pixel_widths = std::env::var("ORACLE_PIXELS").is_ok();
    let width = |text: &str| -> f64 {
        text.chars()
            .map(|ch| {
                let points = f64::from(face.glyph(ch).map_or(0, |g| g.1)) * face.scale(size);
                if pixel_widths {
                    (points / 0.12).round() * 0.12
                } else {
                    points
                }
            })
            .sum()
    };
    let path = out_dir().join(&against).join(format!("{name}_docx.pdf"));
    let library_lock = pdf_glyphs::pdfium();
    let document = library_lock.0.load_pdf_from_file(&path, None).unwrap();
    let mut overflows = Vec::new();
    for page in document.pages().iter() {
        let text = page.text().unwrap().all();
        let lines: Vec<&str> = text
            .split(['\r', '\n'])
            .filter(|l| !l.trim().is_empty())
            .collect();
        for (i, line) in lines.iter().enumerate() {
            // next line's first word: did it fit too?
            let next_word = lines
                .get(i + 1)
                .and_then(|l| l.split_whitespace().next())
                .unwrap_or("");
            let w = width(line.trim_end());
            let with_next = w + width(" ") + width(next_word);
            let spaces = line
                .trim_end()
                .chars()
                .filter(|c| c.is_whitespace())
                .count() as f64
                * width(" ");
            let room = if i == 0 || lines[i - 1].trim_end().ends_with('.') && false {
                432.0
            } else {
                468.0
            };
            let _ = room;
            let last_word = line.split_whitespace().last().unwrap_or("");
            println!(
                "feat over {:7.3} spaces {:3} lastword {:7.3} overnext {:7.3} nextword {:7.3}",
                w - 468.0,
                line.trim_end()
                    .chars()
                    .filter(|c| c.is_whitespace())
                    .count(),
                width(last_word),
                with_next - 468.0,
                width(next_word)
            );
            println!(
                "ratio {:7.3} next {:7.3} {}",
                (w - 468.0) / spaces,
                (with_next - 468.0) / (spaces + width(" ")),
                line.chars().take(30).collect::<String>()
            );
            overflows.push((w, with_next, line.chars().take(40).collect::<String>()));
        }
    }
    for (w, with_next, text) in &overflows {
        println!("{w:8.2} {with_next:8.2} {text}");
    }
}

/// prints every line of one of our pages (ORACLE_ONLY, ORACLE_PAGE).
#[test]
#[ignore = "diagnostic"]
fn print_our_page() {
    use letters_layout::{layout, Flavor, LayoutOptions};
    let name = std::env::var("ORACLE_ONLY").expect("ORACLE_ONLY");
    let page: usize = std::env::var("ORACLE_PAGE")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(0);
    let bytes = std::fs::read(out_dir().join("corpus").join(format!("{name}.docx"))).expect("read");
    let document = letters_office::open(&bytes).expect("open");
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
    let bytes = std::fs::read(out_dir().join("corpus").join(format!("{name}.docx"))).expect("read");
    let document = letters_office::open(&bytes).expect("open");
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

/// the share of lines breaking like libreoffice's over the whole corpus,
/// for fitting the libreoffice flavor (no report written).
#[test]
#[ignore = "fitting"]
fn libreoffice_agreement() {
    use letters_layout::{layout, Flavor, LayoutOptions};
    let corpus_folder = out_dir().join("corpus");
    let mut scores = Vec::new();
    for entry in std::fs::read_dir(&corpus_folder).expect("corpus").flatten() {
        let path = entry.path();
        if path.extension().is_none_or(|ext| ext != "docx") {
            continue;
        }
        let name = path.file_stem().unwrap().to_string_lossy().to_string();
        let document = letters_office::open(&std::fs::read(&path).unwrap()).unwrap();
        let reference = oracle_lines("lo", &name, "docx");
        let ours = Some(score::lines_of_result(&layout(
            &document,
            &LayoutOptions::flavor(Flavor::LibreOffice),
        )));
        if let Some(s) = compare(&reference, &ours, body_zone(&document)) {
            scores.push(s);
        }
    }
    let refs: Vec<&score::Score> = scores.iter().collect();
    let total = score::combine(&refs);
    println!(
        "libreoffice agreement: same lines {:.1}% page breaks {:.1}% baseline mean {:.2}",
        total.same_line_share() * 100.0,
        total.page_break_share() * 100.0,
        total.baseline.mean
    );
}

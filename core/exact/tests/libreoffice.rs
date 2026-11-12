//! how far the exact docx is from the pdf when libreoffice opens it: each
//! corpus docx (easy to edit and locked layout) exported to pdf by a
//! headless libreoffice with a private profile, measured with the same
//! verifier as word's export. only runs when asked:
//!
//!     cargo test -p letters-exact --release --test libreoffice -- --ignored --nocapture
//!
//! the easy to edit docx files come from the word corpus run
//! (`tests/word_hybrid.rs`) when it left them, else they are converted here
//! without word (promotions predicted).

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

use letters_exact::pdf::PdfFile;
use letters_exact::verify::{compare_glyphs, placed_glyphs};
use letters_exact::{convert, ConvertOptions, OutputMode};

/// libreoffice's command line program: LETTERS_SOFFICE, else the usual
/// install folders on each system (the same places as
/// letters_office::bridge::libreoffice_path, which this crate doesn't use)
fn soffice() -> std::path::PathBuf {
    let mut candidates: Vec<std::path::PathBuf> = Vec::new();
    if let Ok(path) = std::env::var("LETTERS_SOFFICE") {
        candidates.push(path.into());
    }
    if let Ok(local) = std::env::var("LOCALAPPDATA") {
        candidates
            .push(std::path::Path::new(&local).join(r"Programs\LibreOffice\program\soffice.com"));
    }
    candidates.push(r"C:\Program Files\LibreOffice\program\soffice.com".into());
    candidates.push("/Applications/LibreOffice.app/Contents/MacOS/soffice".into());
    candidates.push("/usr/bin/soffice".into());
    candidates
        .into_iter()
        .find(|path| path.is_file())
        .expect("libreoffice was not found; set LETTERS_SOFFICE to its soffice program")
}
/// the longest one export may take before it is stopped.
const EXPORT_LIMIT: Duration = Duration::from_secs(180);

const DOCUMENTS: [(&str, &str); 13] = [
    ("syllabus", "syllabus"),
    ("research_paper", "research paper"),
    ("lab_report", "lab report"),
    ("slides", "slides"),
    ("form", "form"),
    ("journal", "journal page"),
    ("cid_subset", "cid subset > 256 glyphs"),
    ("type3_font", "Type 3 font"),
    ("mixed_styles", "mixed styles"),
    ("tj_kerning", "justified TJ kerning"),
    ("cff_font", "CFF font"),
    ("type1_font", "Type 1 font"),
    ("restricted_font", "restricted fsType"),
];

fn manifest_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn out_dir() -> PathBuf {
    let dir = manifest_dir()
        .join("..")
        .join("..")
        .join("target-exact")
        .join("word-tests")
        .join("libreoffice");
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// a file url for libreoffice's -env:UserInstallation.
fn file_url(path: &Path) -> String {
    let absolute = std::path::absolute(path).unwrap();
    format!(
        "file:///{}",
        absolute.display().to_string().replace('\\', "/")
    )
}

/// exports a docx to pdf with a private libreoffice profile; the pdf, or why not.
fn export_with_libreoffice(docx: &Path, dir: &Path) -> Result<PathBuf, String> {
    let profile = dir.join("profile");
    let mut child = Command::new(soffice())
        .arg(format!("-env:UserInstallation={}", file_url(&profile)))
        .args([
            "--headless",
            "--norestore",
            "--nologo",
            "--convert-to",
            "pdf",
            "--outdir",
        ])
        .arg(dir)
        .arg(docx)
        .spawn()
        .map_err(|e| format!("libreoffice could not be started ({e})"))?;
    let started = Instant::now();
    loop {
        if let Some(status) = child.try_wait().map_err(|e| e.to_string())? {
            if !status.success() {
                return Err(format!("libreoffice ended with {status}"));
            }
            break;
        }
        if started.elapsed() > EXPORT_LIMIT {
            let _ = child.kill();
            return Err("libreoffice took too long and was stopped".into());
        }
        std::thread::sleep(Duration::from_millis(200));
    }
    let pdf = dir.join(docx.with_extension("pdf").file_name().unwrap());
    pdf.is_file()
        .then_some(pdf)
        .ok_or_else(|| "no pdf was written".into())
}

/// the docx of one mode: the word run's when there is one, else made here.
fn docx_for(name: &str, mode: OutputMode, dir: &Path) -> PathBuf {
    let from_word_run = manifest_dir()
        .join("..")
        .join("..")
        .join("target-exact")
        .join("word-tests")
        .join("hybrid-corpus")
        .join(format!("{name}.docx"));
    if mode == OutputMode::EasyToEdit && from_word_run.is_file() {
        let copy = dir.join(format!("{name}.edit.docx"));
        std::fs::copy(&from_word_run, &copy).unwrap();
        return copy;
    }
    let suffix = if mode == OutputMode::EasyToEdit {
        "edit"
    } else {
        "locked"
    };
    let docx = dir.join(format!("{name}.{suffix}.docx"));
    let options = ConvertOptions {
        use_word: false,
        mode,
        ..ConvertOptions::default()
    };
    let pdf = manifest_dir()
        .join("tests")
        .join("corpus")
        .join(format!("{name}.pdf"));
    convert(&pdf, &docx, &options).unwrap();
    docx
}

#[test]
#[ignore = "starts libreoffice (a minute or two for the corpus)"]
fn the_corpus_opened_in_libreoffice() {
    soffice();
    let dir = out_dir();
    let mut rows = Vec::new();
    for (name, class) in DOCUMENTS {
        let source = manifest_dir()
            .join("tests")
            .join("corpus")
            .join(format!("{name}.pdf"));
        let source_glyphs = placed_glyphs(&PdfFile::open(&source).unwrap().read_pages());
        let mut cells = Vec::new();
        for mode in [OutputMode::EasyToEdit, OutputMode::LockedLayout] {
            let docx = docx_for(name, mode, &dir);
            let cell = match export_with_libreoffice(&docx, &dir) {
                Ok(pdf) => {
                    let pages = PdfFile::open(&pdf).unwrap().read_pages();
                    let stats = compare_glyphs(&source_glyphs, &placed_glyphs(&pages));
                    format!(
                        "{:.2} / {:.3} / {:.1}% ({} of {})",
                        stats.max_pt,
                        stats.mean_pt,
                        100.0 * stats.within_0_125,
                        stats.glyphs,
                        stats.source_glyphs
                    )
                }
                Err(why) => format!("not exported: {why}"),
            };
            cells.push(cell);
        }
        let row = format!("| {class} | {} | {} |", cells[0], cells[1]);
        println!("{row}");
        rows.push(row);
    }
    println!("| class | easy to edit: max / mean / within 0.125 pt (glyphs measured) | locked layout: the same |");
    for row in rows {
        println!("{row}");
    }
}

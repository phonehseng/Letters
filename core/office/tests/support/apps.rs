//! what the application suites need: the corpus made by word and by
//! libreoffice, rendering through both, and measuring two renderings
//! against each other (glyph positions, object rectangles, page count).
//! word and libreoffice are only ever driven through the crate's bridge,
//! which starts private instances and stops only those.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::process::Command;

use letters_exact::pdf::{PageContent, PdfFile, Segment};
use letters_exact::verify::{compare_glyphs, placed_glyphs, GlyphStats};
use letters_office::bridge::{libreoffice_convert_files, word_batch, WordJob, WordJobKind};

/// every word and libreoffice running now: image name, process id and
/// whether it is a libreoffice using this test's private profile.
pub fn office_processes() -> BTreeSet<(String, u32, bool)> {
    let marker = format!("letters-libreoffice-profile-{}", std::process::id());
    let output = Command::new("powershell")
        .args([
            "-NoProfile",
            "-Command",
            "Get-CimInstance Win32_Process | Where-Object { $_.Name -in 'WINWORD.EXE','soffice.bin','soffice.exe' } | ForEach-Object { \"$($_.Name)|$($_.ProcessId)|$($_.CommandLine)\" }",
        ])
        .output()
        .expect("powershell");
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter_map(|line| {
            let mut fields = line.splitn(3, '|');
            let name = fields.next()?.to_string();
            let pid = fields.next()?.trim().parse().ok()?;
            let ours = fields
                .next()
                .is_some_and(|command| command.contains(&marker));
            Some((name, pid, ours))
        })
        .collect()
}

/// fails when a libreoffice this test started is still running. the
/// bridge stops each private word itself (it checks the process is the
/// one it started); other programs on the machine may start their own
/// word meanwhile, so new word processes are only reported.
pub fn assert_none_left(before: &BTreeSet<(String, u32, bool)>) {
    // soffice.com returns a moment before the soffice.bin it started has
    // finished closing, so it gets a few seconds to end
    for _ in 0..40 {
        if !office_processes().iter().any(|(_, _, ours)| *ours) {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(500));
    }
    let after = office_processes();
    let new: Vec<_> = after.difference(before).collect();
    if !new.is_empty() {
        eprintln!("office processes that were not running before: {new:?}");
    }
    let ours: Vec<_> = after.iter().filter(|(_, _, ours)| *ours).collect();
    assert!(ours.is_empty(), "libreoffice left running: {ours:?}");
}
pub fn work_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join("letters-freeform").join(name);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// a 160 by 120 picture: green with a gold disc, as png.
pub fn picture() -> Vec<u8> {
    let (width, height) = (160u32, 120u32);
    let row_bytes = (width * 3).div_ceil(4) * 4;
    let mut bmp = Vec::new();
    let size = 54 + row_bytes * height;
    bmp.extend(b"BM");
    bmp.extend(size.to_le_bytes());
    bmp.extend([0u8; 4]);
    bmp.extend(54u32.to_le_bytes());
    bmp.extend(40u32.to_le_bytes());
    bmp.extend((width as i32).to_le_bytes());
    bmp.extend((height as i32).to_le_bytes());
    bmp.extend(1u16.to_le_bytes());
    bmp.extend(24u16.to_le_bytes());
    bmp.extend([0u8; 24]);
    for y in 0..height {
        let mut row = Vec::new();
        for x in 0..width {
            let (dx, dy) = (x as f64 - 80.0, y as f64 - 60.0);
            let inside = (dx * dx) / 2500.0 + (dy * dy) / 1600.0 <= 1.0;
            // bmp stores blue, green, red
            row.extend(if inside {
                [0u8, 200, 250]
            } else {
                [90u8, 140, 40]
            });
        }
        row.resize(row_bytes as usize, 0);
        bmp.extend(row);
    }
    letters_office::clipboard::bmp_to_png(&bmp).unwrap()
}

/// the word corpus, made once per machine (word takes about twenty
/// seconds for all six).
pub fn word_corpus() -> Vec<(String, PathBuf)> {
    let dir = work_dir("word-corpus");
    let names = [
        "shapes",
        "groups",
        "floating_table",
        "linked_boxes",
        "columns",
        "flyer",
    ];
    let missing = names
        .iter()
        .any(|name| !dir.join(format!("{name}.docx")).is_file());
    if missing {
        let picture_path = dir.join("picture.png");
        std::fs::write(&picture_path, picture()).unwrap();
        let script = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests")
            .join("tools")
            .join("word_corpus.ps1");
        let status = Command::new("powershell")
            .args([
                "-NoProfile",
                "-NonInteractive",
                "-ExecutionPolicy",
                "Bypass",
                "-File",
            ])
            .arg(&script)
            .arg("-OutDir")
            .arg(&dir)
            .arg("-Picture")
            .arg(&picture_path)
            .status()
            .unwrap();
        assert!(status.success(), "word could not make the corpus");
    }
    names
        .iter()
        .map(|name| (format!("word_{name}"), dir.join(format!("{name}.docx"))))
        .collect()
}

/// the libreoffice corpus: flat odt saved by libreoffice as .odt.
pub fn libreoffice_corpus() -> Vec<(String, PathBuf)> {
    let dir = work_dir("libreoffice-corpus");
    let base64 = letters_office::base64::encode(&picture());
    let documents = super::lo_corpus::documents(&base64);
    let mut flat = Vec::new();
    for (name, xml) in &documents {
        let path = dir.join(format!("{name}.fodt"));
        std::fs::write(&path, xml).unwrap();
        flat.push(path);
    }
    let odt_dir = dir.join("odt");
    if documents
        .iter()
        .any(|(name, _)| !odt_dir.join(format!("{name}.odt")).is_file())
    {
        libreoffice_convert_files(&flat, "odt", &odt_dir).unwrap();
    }
    documents
        .iter()
        .map(|(name, _)| (name.to_string(), odt_dir.join(format!("{name}.odt"))))
        .collect()
}

/// pdfs of `files` as word prints them, one word for all of them.
pub fn word_pdfs(files: &[PathBuf], out_dir: &Path) -> Vec<Option<Vec<u8>>> {
    std::fs::create_dir_all(out_dir).unwrap();
    let jobs: Vec<WordJob> = files
        .iter()
        .enumerate()
        .map(|(index, file)| WordJob {
            input: file.clone(),
            output: out_dir.join(format!(
                "{index}-{}.pdf",
                file.file_stem().unwrap().to_string_lossy()
            )),
            kind: WordJobKind::Pdf,
        })
        .collect();
    let mut outcomes =
        word_batch(&jobs).unwrap_or_else(|error| vec![Err(error.to_string()); jobs.len()]);
    // a file that makes word fail takes the rest of the batch with it, so
    // the ones that failed get one more try each in a word of their own
    for (job, outcome) in jobs.iter().zip(outcomes.iter_mut()) {
        if outcome.is_err() {
            if let Ok(mut again) = word_batch(std::slice::from_ref(job)) {
                *outcome = again.remove(0);
            }
        }
    }
    jobs.iter()
        .zip(outcomes)
        .map(|(job, outcome)| match outcome {
            Ok(()) => std::fs::read(&job.output).ok(),
            Err(error) => {
                eprintln!("word could not export {}: {error}", job.input.display());
                None
            }
        })
        .collect()
}

/// word saving each of `files` in another format.
pub fn word_saves(
    files: &[PathBuf],
    format: u32,
    extension: &str,
    out_dir: &Path,
) -> Vec<Option<PathBuf>> {
    std::fs::create_dir_all(out_dir).unwrap();
    let jobs: Vec<WordJob> = files
        .iter()
        .map(|file| WordJob {
            input: file.clone(),
            output: out_dir.join(format!(
                "{}.{extension}",
                file.file_stem().unwrap().to_string_lossy()
            )),
            kind: WordJobKind::Convert { format },
        })
        .collect();
    let outcomes = word_batch(&jobs).expect("word batch");
    jobs.iter()
        .zip(outcomes)
        .map(|(job, outcome)| outcome.ok().map(|_| job.output.clone()))
        .collect()
}

/// libreoffice converting `files` (copied under unique names first, as
/// libreoffice names its output after the input).
pub fn libreoffice_outputs(
    files: &[PathBuf],
    format: &str,
    extension: &str,
    out_dir: &Path,
) -> Vec<Option<PathBuf>> {
    let staging = out_dir.join("in");
    std::fs::create_dir_all(&staging).unwrap();
    let mut inputs = Vec::new();
    for (index, file) in files.iter().enumerate() {
        let name = format!("{index}-{}", file.file_name().unwrap().to_string_lossy());
        let copy = staging.join(name);
        std::fs::copy(file, &copy).unwrap();
        inputs.push(copy);
    }
    libreoffice_convert_files(&inputs, format, out_dir).expect("libreoffice");
    inputs
        .iter()
        .map(|input| {
            let produced = out_dir.join(format!(
                "{}.{extension}",
                input.file_stem().unwrap().to_string_lossy()
            ));
            produced.is_file().then_some(produced)
        })
        .collect()
}

pub fn libreoffice_pdfs(files: &[PathBuf], out_dir: &Path) -> Vec<Option<Vec<u8>>> {
    libreoffice_outputs(files, "pdf", "pdf", out_dir)
        .into_iter()
        .map(|path| path.and_then(|p| std::fs::read(p).ok()))
        .collect()
}

/// a filled or stroked shape or a picture, as its box on its page (pt).
#[derive(Clone, Debug)]
pub struct ObjectBox {
    pub page: usize,
    pub picture: bool,
    pub rect: [f64; 4],
}

pub struct Rendering {
    pub pages: Vec<PageContent>,
    pub objects: Vec<ObjectBox>,
}

pub fn read_rendering(pdf: &[u8]) -> Rendering {
    let file = PdfFile::from_bytes(pdf).expect("a pdf");
    let pages = file.read_pages();
    let mut objects = Vec::new();
    for page in &pages {
        for path in &page.paths {
            if !(path.fill || path.stroke) {
                continue;
            }
            let points: Vec<(f64, f64)> = path.segments.iter().flat_map(Segment::points).collect();
            if let Some(rect) = bounds(&points) {
                objects.push(ObjectBox {
                    page: page.index,
                    picture: false,
                    rect,
                });
            }
        }
        for image in &page.images {
            let m = image.matrix;
            let corners: Vec<(f64, f64)> = [(0.0, 0.0), (1.0, 0.0), (0.0, 1.0), (1.0, 1.0)]
                .iter()
                .map(|(x, y)| letters_exact::pdf::matrix::apply(&m, *x, *y))
                .collect();
            if let Some(rect) = bounds(&corners) {
                objects.push(ObjectBox {
                    page: page.index,
                    picture: true,
                    rect,
                });
            }
        }
    }
    Rendering { pages, objects }
}

fn bounds(points: &[(f64, f64)]) -> Option<[f64; 4]> {
    let first = points.first()?;
    let mut rect = [first.0, first.1, first.0, first.1];
    for (x, y) in points {
        rect[0] = rect[0].min(*x);
        rect[1] = rect[1].min(*y);
        rect[2] = rect[2].max(*x);
        rect[3] = rect[3].max(*y);
    }
    Some(rect)
}

/// how far apart two renderings are.
#[derive(Clone, Debug)]
pub struct Difference {
    pub pages: (usize, usize),
    pub glyphs: GlyphStats,
    pub objects: (usize, usize),
    pub objects_matched: usize,
    pub object_max_pt: f64,
    pub object_mean_pt: f64,
}

impl Difference {
    /// the same in every measured way, within `tolerance` points.
    pub fn identical(&self, tolerance: f64) -> bool {
        self.pages.0 == self.pages.1
            && self.glyphs.source_glyphs == self.glyphs.word_glyphs
            && self.glyphs.matched == self.glyphs.source_glyphs
            && self.glyphs.max_pt <= tolerance
            && self.objects.0 == self.objects.1
            && self.objects_matched == self.objects.0
            && self.object_max_pt <= tolerance
    }

    pub fn summary(&self) -> String {
        format!(
            "pages {}/{}, glyphs {}/{} matched {} (max {:.3} pt, mean {:.4} pt), objects {}/{} matched {} (max {:.3} pt, mean {:.4} pt)",
            self.pages.0,
            self.pages.1,
            self.glyphs.source_glyphs,
            self.glyphs.word_glyphs,
            self.glyphs.glyphs,
            self.glyphs.max_pt,
            self.glyphs.mean_pt,
            self.objects.0,
            self.objects.1,
            self.objects_matched,
            self.object_max_pt,
            self.object_mean_pt
        )
    }
}

/// how far `b` is from `a`. glyphs are paired in reading order by their
/// text; objects by the nearest box of the same kind on the same page.
pub fn compare(a: &Rendering, b: &Rendering) -> Difference {
    let glyphs = compare_glyphs(&placed_glyphs(&a.pages), &placed_glyphs(&b.pages));
    let mut used = vec![false; b.objects.len()];
    let mut distances = Vec::new();
    for object in &a.objects {
        let best = b
            .objects
            .iter()
            .enumerate()
            .filter(|(index, other)| {
                !used[*index] && other.page == object.page && other.picture == object.picture
            })
            .map(|(index, other)| {
                let gap = (0..4)
                    .map(|k| (other.rect[k] - object.rect[k]).abs())
                    .fold(0.0, f64::max);
                (index, gap)
            })
            .min_by(|x, y| x.1.total_cmp(&y.1));
        // a box more than half an inch away is another object
        if let Some((index, gap)) = best.filter(|(_, gap)| *gap < 36.0) {
            used[index] = true;
            distances.push(gap);
        }
    }
    let count = distances.len().max(1) as f64;
    Difference {
        pages: (a.pages.len(), b.pages.len()),
        glyphs,
        objects: (a.objects.len(), b.objects.len()),
        objects_matched: distances.len(),
        object_max_pt: distances.iter().copied().fold(0.0, f64::max),
        object_mean_pt: distances.iter().sum::<f64>() / count,
    }
}

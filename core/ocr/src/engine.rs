//! running tesseract, the program that does the actual reading.
//!
//! tesseract runs as a separate program rather than a library linked into
//! Letters (see README.md for why). each page is handed to it through its
//! standard input as an uncompressed gray picture, and it answers with
//! hocr on its standard output, so no temporary files are needed. if
//! tesseract crashes on a strange page, only that page fails.

use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::thread;
use std::time::Duration;

use image::GrayImage;

use crate::cancel::CancelFlag;
use crate::languages::{check_codes, language_name};
use crate::OcrError;

/// how often a running tesseract is checked for being finished or cancelled.
const POLL_INTERVAL: Duration = Duration::from_millis(10);

/// the data file that lets tesseract tell which way up a page is.
const ORIENTATION_DATA: &str = "osd";

#[cfg(windows)]
const PROGRAM_NAME: &str = "tesseract.exe";
#[cfg(not(windows))]
const PROGRAM_NAME: &str = "tesseract";

/// where tesseract and its language data are.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Engine {
    program: PathBuf,
    tessdata: PathBuf,
}

/// what tesseract's orientation detection found.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct OrientationGuess {
    /// quarter turns clockwise that make the page upright
    pub quarter_turns: u32,
    /// tesseract's own confidence; above about 2 it is usually right
    pub confidence: f32,
}

impl Engine {
    /// finds tesseract in a folder laid out like vendor/tesseract:
    /// the program in `bin`, the language data in `tessdata`.
    pub fn locate(folder: &Path) -> Result<Self, OcrError> {
        Self::new(
            folder.join("bin").join(PROGRAM_NAME),
            folder.join("tessdata"),
        )
    }

    pub fn new(program: PathBuf, tessdata: PathBuf) -> Result<Self, OcrError> {
        if !program.is_file() {
            return Err(OcrError::EngineMissing(format!(
                "{} was not found",
                program.display()
            )));
        }
        if !tessdata.is_dir() {
            return Err(OcrError::EngineMissing(format!(
                "{} was not found",
                tessdata.display()
            )));
        }
        Ok(Self { program, tessdata })
    }

    /// tesseract's version line, such as "tesseract 5.5.4".
    pub fn version(&self) -> Result<String, OcrError> {
        let output = self.run(&["--version".into()], &[], &CancelFlag::new())?;
        let text = String::from_utf8_lossy(&output.stdout);
        Ok(text.lines().next().unwrap_or_default().trim().to_owned())
    }

    /// the codes of every language whose data is installed, sorted.
    /// "osd" is left out: it finds orientation, it is not a language.
    pub fn installed_languages(&self) -> Vec<String> {
        let Ok(entries) = std::fs::read_dir(&self.tessdata) else {
            return Vec::new();
        };
        let mut codes: Vec<String> = entries
            .filter_map(|entry| {
                let name = entry.ok()?.file_name().into_string().ok()?;
                let code = name.strip_suffix(".traineddata")?;
                (code != ORIENTATION_DATA).then(|| code.to_owned())
            })
            .collect();
        codes.sort();
        codes
    }

    /// whether the orientation data is installed.
    pub fn can_detect_orientation(&self) -> bool {
        self.data_file(ORIENTATION_DATA).is_file()
    }

    /// checks the codes and that each language's data is installed, so a
    /// missing one is reported by name before any work is done.
    pub fn check_languages(&self, codes: &[String]) -> Result<(), OcrError> {
        check_codes(codes)?;
        for code in codes {
            if !self.data_file(code).is_file() {
                return Err(OcrError::MissingLanguage {
                    code: code.clone(),
                    name: language_name(code),
                });
            }
        }
        Ok(())
    }

    fn data_file(&self, code: &str) -> PathBuf {
        self.tessdata.join(format!("{code}.traineddata"))
    }

    /// reads the text of a cleaned page picture and returns tesseract's hocr.
    pub(crate) fn read_hocr(
        &self,
        image: &GrayImage,
        dpi: f32,
        languages: &[String],
        cancel: &CancelFlag,
    ) -> Result<String, OcrError> {
        self.check_languages(languages)?;
        let mut arguments = self.common_arguments(dpi);
        arguments.extend([
            "-l".into(),
            languages.join("+"),
            // fully automatic page layout: columns, paragraphs, lines
            "--psm".into(),
            "3".into(),
            "-c".into(),
            "tessedit_create_hocr=1".into(),
            "-c".into(),
            "tessedit_create_txt=0".into(),
        ]);
        let output = self.run(&arguments, &pgm_bytes(image), cancel)?;
        if !output.status.success() {
            return Err(OcrError::EngineFailed(last_line(&output.stderr)));
        }
        String::from_utf8(output.stdout)
            .map_err(|_| OcrError::EngineFailed("the answer was not valid text".into()))
    }

    /// asks tesseract which way up the page is. none when it can't tell,
    /// which happens on pages with little text.
    pub(crate) fn detect_orientation(
        &self,
        image: &GrayImage,
        dpi: f32,
        cancel: &CancelFlag,
    ) -> Result<Option<OrientationGuess>, OcrError> {
        if !self.can_detect_orientation() {
            return Ok(None);
        }
        let mut arguments = self.common_arguments(dpi);
        // page layout mode 0 only detects orientation and script
        arguments.extend(["--psm".into(), "0".into()]);
        let output = self.run(&arguments, &pgm_bytes(image), cancel)?;
        if !output.status.success() {
            return Ok(None);
        }
        Ok(parse_orientation(&String::from_utf8_lossy(&output.stdout)))
    }

    fn common_arguments(&self, dpi: f32) -> Vec<String> {
        vec![
            "stdin".into(),
            "stdout".into(),
            "--tessdata-dir".into(),
            self.tessdata.to_string_lossy().into_owned(),
            // the picture arrives without a resolution, so say it
            "--dpi".into(),
            (dpi.round() as u32).max(1).to_string(),
        ]
    }

    /// runs tesseract with `input` on its standard input and collects what
    /// it prints. stops it straight away if `cancel` is flipped.
    fn run(
        &self,
        arguments: &[String],
        input: &[u8],
        cancel: &CancelFlag,
    ) -> Result<Output, OcrError> {
        if cancel.is_cancelled() {
            return Err(OcrError::Cancelled);
        }
        let mut command = Command::new(&self.program);
        command
            .args(arguments)
            // the language data is named on the command line; a stray
            // TESSDATA_PREFIX from another install must not win
            .env_remove("TESSDATA_PREFIX")
            // tesseract may use several threads per page; one per process
            // keeps the app responsive and the timing predictable
            .env("OMP_THREAD_LIMIT", "1")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        hide_console_window(&mut command);
        let mut child = command.spawn().map_err(|error| {
            OcrError::EngineMissing(format!("tesseract could not start: {error}"))
        })?;

        // feed and drain on their own threads: tesseract can't finish until
        // it has all of its input, and it blocks if its output isn't read
        let mut stdin = child.stdin.take().expect("stdin was piped");
        let input = input.to_vec();
        let feeder = thread::spawn(move || {
            // a broken pipe here means tesseract stopped early; its exit
            // status and message say why, so the write error adds nothing
            let _ = stdin.write_all(&input);
        });
        let stdout = drain(child.stdout.take().expect("stdout was piped"));
        let stderr = drain(child.stderr.take().expect("stderr was piped"));

        let status = wait_or_cancel(&mut child, cancel)?;
        let _ = feeder.join();
        Ok(Output {
            status,
            stdout: stdout.join().unwrap_or_default(),
            stderr: stderr.join().unwrap_or_default(),
        })
    }
}

struct Output {
    status: ExitStatus,
    stdout: Vec<u8>,
    stderr: Vec<u8>,
}

fn drain(mut pipe: impl Read + Send + 'static) -> thread::JoinHandle<Vec<u8>> {
    thread::spawn(move || {
        let mut bytes = Vec::new();
        let _ = pipe.read_to_end(&mut bytes);
        bytes
    })
}

fn wait_or_cancel(child: &mut Child, cancel: &CancelFlag) -> Result<ExitStatus, OcrError> {
    loop {
        if let Some(status) = child
            .try_wait()
            .map_err(|error| OcrError::EngineFailed(error.to_string()))?
        {
            return Ok(status);
        }
        if cancel.is_cancelled() {
            let _ = child.kill();
            let _ = child.wait();
            return Err(OcrError::Cancelled);
        }
        thread::sleep(POLL_INTERVAL);
    }
}

#[cfg(windows)]
fn hide_console_window(command: &mut Command) {
    use std::os::windows::process::CommandExt;
    // CREATE_NO_WINDOW: without it a black console window flashes up for
    // every page when Letters runs as a normal windowed app
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    command.creation_flags(CREATE_NO_WINDOW);
}

#[cfg(not(windows))]
fn hide_console_window(_command: &mut Command) {}

/// a gray picture in the "pgm" format: a three line text header and then
/// the raw pixels. tesseract reads it without any decoding work.
fn pgm_bytes(image: &GrayImage) -> Vec<u8> {
    let header = format!("P5\n{} {}\n255\n", image.width(), image.height());
    let mut bytes = Vec::with_capacity(header.len() + image.as_raw().len());
    bytes.extend_from_slice(header.as_bytes());
    bytes.extend_from_slice(image.as_raw());
    bytes
}

/// the last thing tesseract complained about, for the error message.
fn last_line(stderr: &[u8]) -> String {
    let text = String::from_utf8_lossy(stderr);
    text.lines()
        .map(str::trim)
        .rfind(|line| !line.is_empty())
        .unwrap_or("tesseract stopped without saying why")
        .to_owned()
}

/// reads tesseract's orientation report:
///
/// ```text
/// Orientation in degrees: 270
/// Rotate: 90
/// Orientation confidence: 3.21
/// ```
///
/// "Rotate" is how far the picture must be turned clockwise to be upright.
fn parse_orientation(report: &str) -> Option<OrientationGuess> {
    let value = |name: &str| {
        report.lines().find_map(|line| {
            let (key, value) = line.split_once(':')?;
            (key.trim() == name).then(|| value.trim().parse::<f32>().ok())?
        })
    };
    let rotate = value("Rotate")? as i64;
    let confidence = value("Orientation confidence").unwrap_or(0.0);
    Some(OrientationGuess {
        quarter_turns: (rotate.rem_euclid(360) / 90) as u32,
        confidence,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pgm_has_a_header_then_the_pixels() {
        let image = GrayImage::from_raw(2, 1, vec![7, 9]).unwrap();
        assert_eq!(pgm_bytes(&image), b"P5\n2 1\n255\n\x07\x09");
    }

    #[test]
    fn reads_the_orientation_report() {
        let report = "Page number: 0\nOrientation in degrees: 270\nRotate: 90\nOrientation confidence: 3.21\nScript: Latin\n";
        let guess = parse_orientation(report).unwrap();
        assert_eq!(guess.quarter_turns, 1);
        assert!((guess.confidence - 3.21).abs() < 1e-4);
        assert_eq!(parse_orientation("Too few characters"), None);
    }

    #[test]
    fn a_missing_program_is_reported() {
        let missing = Engine::locate(Path::new("no such folder"));
        assert!(matches!(missing, Err(OcrError::EngineMissing(_))));
    }

    #[test]
    fn the_last_complaint_is_kept() {
        assert_eq!(last_line(b"warning\nfailed loading\n\n"), "failed loading");
        assert_eq!(last_line(b""), "tesseract stopped without saying why");
    }
}

//! libreoffice, on any system: `soffice --headless --convert-to pdf`.
//!
//! it is given a profile of its own in the temporary folder. without one,
//! a libreoffice the person already has open would be handed the job (or
//! the run would wait for it), and its settings and recent files changed.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use letters_ocr::CancelFlag;

use super::process::{run_with_limit, RunEnd};
use crate::ConvertError;

/// where libreoffice is installed, if it is.
pub(super) fn find_program() -> Option<PathBuf> {
    let mut places: Vec<PathBuf> = Vec::new();
    if cfg!(windows) {
        for variable in ["ProgramFiles", "ProgramFiles(x86)", "ProgramW6432"] {
            if let Some(folder) = std::env::var_os(variable) {
                places.push(
                    Path::new(&folder)
                        .join("LibreOffice")
                        .join("program")
                        .join("soffice.exe"),
                );
            }
        }
    } else if cfg!(target_os = "macos") {
        places.push("/Applications/LibreOffice.app/Contents/MacOS/soffice".into());
    } else {
        places.push("/usr/bin/soffice".into());
        places.push("/usr/lib/libreoffice/program/soffice".into());
        places.push("/opt/libreoffice/program/soffice".into());
        places.push("/snap/bin/libreoffice".into());
    }
    places.into_iter().find(|path| path.is_file()).or_else(|| {
        on_path(if cfg!(windows) {
            "soffice.exe"
        } else {
            "soffice"
        })
    })
}

fn on_path(program: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .map(|folder| folder.join(program))
        .find(|candidate| candidate.is_file())
}

pub(super) fn convert(
    program: &Path,
    input: &Path,
    work: &Path,
    limit: Duration,
    cancel: &CancelFlag,
) -> Result<(PathBuf, Option<u32>), ConvertError> {
    let input =
        std::path::absolute(input).map_err(|e| ConvertError::CannotOpen(format!("({e})")))?;
    let profile = work.join("profile");
    let out = work.join("out");
    std::fs::create_dir_all(&out).map_err(crate::error::write_failed)?;
    let mut command = Command::new(program);
    command
        .arg("--headless")
        .arg("--invisible")
        .arg("--nologo")
        .arg("--nodefault")
        .arg("--norestore")
        .arg("--nolockcheck")
        .arg(format!("-env:UserInstallation={}", file_url(&profile)))
        .args(["--convert-to", "pdf", "--outdir"])
        .arg(&out)
        .arg(&input);
    let failed = |detail: String| ConvertError::ConverterFailed {
        program: "LibreOffice".into(),
        detail,
    };
    match run_with_limit(command, limit, cancel, |_| None) {
        RunEnd::Finished { errors, .. } => {
            let stem = input
                .file_stem()
                .map(|s| s.to_os_string())
                .unwrap_or_default();
            let made = out.join(Path::new(&stem).with_extension("pdf"));
            if made.is_file() {
                Ok((made, None))
            } else {
                let reason = errors
                    .lines()
                    .map(str::trim)
                    .find(|l| !l.is_empty())
                    .unwrap_or("no reason given");
                Err(failed(format!(
                    "The file may be damaged or locked with a password. ({reason})"
                )))
            }
        }
        RunEnd::TimedOut => Err(ConvertError::TimedOut {
            program: "LibreOffice".into(),
            seconds: limit.as_secs(),
        }),
        RunEnd::Cancelled => Err(ConvertError::Cancelled),
        RunEnd::NotStarted(reason) => Err(failed(format!("It couldn’t be started. ({reason})"))),
    }
}

/// a folder as a file:// address, the way libreoffice wants its profile.
fn file_url(path: &Path) -> String {
    let text = path.to_string_lossy().replace('\\', "/");
    let encoded: String = text
        .chars()
        .map(|c| match c {
            ' ' => "%20".to_string(),
            c => c.to_string(),
        })
        .collect();
    if encoded.starts_with('/') {
        format!("file://{encoded}")
    } else {
        format!("file:///{encoded}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn profiles_are_file_addresses() {
        assert_eq!(file_url(Path::new("/tmp/a b")), "file:///tmp/a%20b");
        if cfg!(windows) {
            assert_eq!(file_url(Path::new("C:\\Temp\\p")), "file:///C:/Temp/p");
        }
    }
}

//! driving the reference applications for the tests: microsoft excel
//! through `tests/tools/excel_check.ps1` (a private hidden instance,
//! stopped by its own process id), and libreoffice calc headless with a
//! private profile that recalculates every file it opens. both are
//! always waited for, with a time limit.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use letters_sheets::model::{ErrorValue, Value};

pub fn excel_available() -> bool {
    Command::new("reg")
        .args(["query", r"HKCR\Excel.Application\CurVer"])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|status| status.success())
}

pub fn libreoffice_path() -> Option<PathBuf> {
    let local = std::env::var("LOCALAPPDATA").ok()?;
    let path = Path::new(&local).join(r"Programs\LibreOffice\program\soffice.com");
    path.is_file().then_some(path)
}

/// a folder for one run, under the crate's target folder so it is easy to inspect
pub fn work_folder(name: &str) -> PathBuf {
    let folder = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../target-sheets/harness")
        .join(name);
    let _ = std::fs::remove_dir_all(&folder);
    std::fs::create_dir_all(&folder).expect("the harness folder can be made");
    folder
}

/// waits for a child process, ending exactly that process tree after `limit`
fn wait(mut child: std::process::Child, limit: Duration, what: &str) -> Result<(), String> {
    let started = Instant::now();
    loop {
        if let Some(status) = child.try_wait().map_err(|e| e.to_string())? {
            return if status.success() {
                Ok(())
            } else {
                Err(format!("{what} ended with {status}"))
            };
        }
        if started.elapsed() > limit {
            let _ = Command::new("taskkill")
                .args(["/T", "/F", "/PID", &child.id().to_string()])
                .output();
            let _ = child.wait();
            return Err(format!(
                "{what} took longer than {} seconds",
                limit.as_secs()
            ));
        }
        std::thread::sleep(Duration::from_millis(100));
    }
}

/// one row of excel's answers: (row, value, shown text)
pub type ExcelRow = (u32, Value, String);

pub fn excel(mode: &str, path: &Path, out: &Path, sheet: &str, column: u32) -> Result<(), String> {
    let script = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/tools/excel_check.ps1");
    let child = Command::new("powershell")
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-ExecutionPolicy",
            "Bypass",
            "-File",
        ])
        .arg(&script)
        .args(["-Mode", mode, "-Path"])
        .arg(path)
        .arg("-Out")
        .arg(out)
        .args(["-Sheet", sheet, "-Column", &column.to_string()])
        .stdout(Stdio::null())
        .stderr(Stdio::inherit())
        .spawn()
        .map_err(|e| e.to_string())?;
    wait(child, Duration::from_secs(900), "Excel")
}

/// excel's value and shown text of every row of one column
pub fn excel_values(path: &Path, sheet: &str, column: u32) -> Result<Vec<ExcelRow>, String> {
    let out = path.with_extension("excel.tsv");
    excel("values", path, &out, sheet, column)?;
    let text = std::fs::read_to_string(&out).map_err(|e| e.to_string())?;
    Ok(text.lines().filter_map(parse_excel_line).collect())
}

fn parse_excel_line(line: &str) -> Option<ExcelRow> {
    let mut parts = line.splitn(4, '\t');
    let row: u32 = parts.next()?.parse().ok()?;
    let kind = parts.next()?;
    let raw = parts.next().unwrap_or("");
    let shown = parts.next().unwrap_or("").to_string();
    let value = match kind {
        "n" => Value::Number(raw.parse().ok()?),
        "b" => Value::Bool(raw == "TRUE"),
        "e" => Value::Error(ErrorValue::parse(raw)?),
        "s" => Value::Text(raw.to_string()),
        _ => Value::Blank,
    };
    Some((row, value, shown))
}

/// a profile folder of our own, set to recalculate every file on loading
/// (libreoffice keeps the stored results of excel files by default)
fn libreoffice_profile() -> PathBuf {
    let profile =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target-sheets/harness/lo-profile");
    let user = profile.join("user");
    std::fs::create_dir_all(&user).expect("the profile folder can be made");
    let settings = r#"<?xml version="1.0" encoding="UTF-8"?>
<oor:items xmlns:oor="http://openoffice.org/2001/registry" xmlns:xs="http://www.w3.org/2001/XMLSchema" xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance">
<item oor:path="/org.openoffice.Office.Calc/Formula/Load"><prop oor:name="OOXMLRecalcMode" oor:op="fuse"><value>0</value></prop></item>
<item oor:path="/org.openoffice.Office.Calc/Formula/Load"><prop oor:name="ODFRecalcMode" oor:op="fuse"><value>0</value></prop></item>
</oor:items>"#;
    let file = user.join("registrymodifications.xcu");
    if !file.is_file() {
        std::fs::write(&file, settings).expect("the profile settings can be written");
    }
    profile
}

/// converts with libreoffice (its --convert-to filter) into `out_folder`
pub fn libreoffice_convert(input: &Path, filter: &str, out_folder: &Path) -> Result<(), String> {
    let program = libreoffice_path().ok_or("LibreOffice is not installed")?;
    let profile = libreoffice_profile();
    let url = format!(
        "file:///{}",
        profile
            .display()
            .to_string()
            .replace('\\', "/")
            .trim_start_matches('/')
    );
    let child = Command::new(program)
        .args(["--headless", "--norestore", "--nolockcheck"])
        .arg(format!("-env:UserInstallation={url}"))
        .args(["--convert-to", filter, "--outdir"])
        .arg(out_folder)
        .arg(input)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| e.to_string())?;
    wait(child, Duration::from_secs(600), "LibreOffice")
}

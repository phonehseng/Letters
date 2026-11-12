//! finds the checkers and editors already installed on this computer.
//!
//! nothing is installed or downloaded. a tool counts as found when its
//! program is on PATH or in one of a few well-known per-user folders, and
//! it answers a version question within a few seconds.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use serde::Serialize;

use super::runner::run_with_limits;

/// a cold java or python start can take a second or two on a slow disk
const VERSION_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ToolKind {
    Gcc,
    Gxx,
    Clang,
    ClangXx,
    Python,
    Javac,
    Node,
    VsCode,
}

impl ToolKind {
    pub const ALL: [ToolKind; 8] = [
        ToolKind::Gcc,
        ToolKind::Gxx,
        ToolKind::Clang,
        ToolKind::ClangXx,
        ToolKind::Python,
        ToolKind::Javac,
        ToolKind::Node,
        ToolKind::VsCode,
    ];

    /// program names to look for, in order of preference.
    fn program_names(self) -> &'static [&'static str] {
        match self {
            ToolKind::Gcc => &["gcc"],
            ToolKind::Gxx => &["g++"],
            ToolKind::Clang => &["clang"],
            ToolKind::ClangXx => &["clang++"],
            ToolKind::Python => &["python3", "python"],
            ToolKind::Javac => &["javac"],
            ToolKind::Node => &["node"],
            ToolKind::VsCode => &["code"],
        }
    }

    /// the arguments that make the program print its version and stop.
    fn version_arguments(self) -> &'static [&'static str] {
        match self {
            ToolKind::Gcc | ToolKind::Gxx => &["-dumpfullversion"],
            ToolKind::Javac => &["-version"],
            _ => &["--version"],
        }
    }

    /// the name shown in "Checked by gcc 16.2".
    fn display_name(self) -> &'static str {
        match self {
            ToolKind::Gcc => "gcc",
            ToolKind::Gxx => "g++",
            ToolKind::Clang => "clang",
            ToolKind::ClangXx => "clang++",
            ToolKind::Python => "Python",
            ToolKind::Javac => "javac",
            ToolKind::Node => "Node.js",
            ToolKind::VsCode => "Visual Studio Code",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FoundTool {
    pub kind: ToolKind,
    pub path: PathBuf,
    /// "gcc 16.2", "Python 3.12.1"
    pub label: String,
}

/// the folders searched, in order: PATH first, then per-user installs that
/// often aren't on PATH (a winlibs mingw unpacked by hand, the per-user
/// node, python and vs code installers, homebrew on a mac).
pub fn search_folders(path_variable: Option<&str>, local_app_data: Option<&Path>) -> Vec<PathBuf> {
    let mut folders: Vec<PathBuf> = path_variable
        .map(|value| std::env::split_paths(value).collect())
        .unwrap_or_default();
    if let Some(local) = local_app_data {
        let programs = local.join("Programs");
        folders.push(programs.join("mingw64").join("bin"));
        folders.push(programs.join("nodejs"));
        folders.push(programs.join("Microsoft VS Code").join("bin"));
        folders.extend(python_folders(&programs.join("Python")));
    }
    if cfg!(windows) {
        folders.push(PathBuf::from(r"C:\Program Files\Microsoft VS Code\bin"));
    }
    if cfg!(target_os = "macos") {
        folders.push(PathBuf::from("/opt/homebrew/bin"));
        folders.push(PathBuf::from("/usr/local/bin"));
        folders.push(PathBuf::from(
            "/Applications/Visual Studio Code.app/Contents/Resources/app/bin",
        ));
    }
    folders
}

/// "Python312", "Python313"... under the per-user python folder, newest first.
fn python_folders(root: &Path) -> Vec<PathBuf> {
    let mut found: Vec<PathBuf> = std::fs::read_dir(root)
        .map(|entries| entries.flatten().map(|entry| entry.path()).collect())
        .unwrap_or_default();
    found.sort();
    found.reverse();
    found
}

/// the first folder holding one of `names` as a program.
pub fn find_program(names: &[&str], folders: &[PathBuf]) -> Option<PathBuf> {
    for name in names {
        for folder in folders {
            for candidate in program_files(folder, name) {
                if candidate.is_file() && !is_store_placeholder(&candidate) {
                    return Some(candidate);
                }
            }
        }
    }
    None
}

// windows runs "gcc" as gcc.exe and "code" as code.cmd; elsewhere the name is the file
fn program_files(folder: &Path, name: &str) -> Vec<PathBuf> {
    if cfg!(windows) {
        vec![
            folder.join(format!("{name}.exe")),
            folder.join(format!("{name}.cmd")),
        ]
    } else {
        vec![folder.join(name)]
    }
}

// windows ships python.exe stubs in WindowsApps that open the Microsoft
// Store instead of running python, so they never count as installed
fn is_store_placeholder(path: &Path) -> bool {
    path.to_string_lossy()
        .to_ascii_lowercase()
        .contains("\\microsoft\\windowsapps\\")
}

/// the version line a tool printed, made short: "16.2.0" becomes "gcc 16.2".
pub fn version_label(kind: ToolKind, printed: &str) -> String {
    let first_line = printed.lines().map(str::trim).find(|line| !line.is_empty());
    let version = first_line
        .and_then(|line| line.split_whitespace().find(|word| starts_with_digit(word)))
        .map(|word| word.trim_start_matches('v'))
        .unwrap_or("");
    let short = shorten_version(kind, version);
    if short.is_empty() {
        kind.display_name().to_owned()
    } else {
        format!("{} {short}", kind.display_name())
    }
}

fn starts_with_digit(word: &str) -> bool {
    word.trim_start_matches('v')
        .chars()
        .next()
        .is_some_and(|c| c.is_ascii_digit())
}

// compilers are named by major and minor ("gcc 16.2"), as their own
// release notes do; python and java keep the patch number people look up
fn shorten_version(kind: ToolKind, version: &str) -> String {
    match kind {
        ToolKind::Gcc | ToolKind::Gxx | ToolKind::Clang | ToolKind::ClangXx => {
            version.split('.').take(2).collect::<Vec<_>>().join(".")
        }
        _ => version.to_owned(),
    }
}

/// asks one tool its version; none when it doesn't answer properly.
fn ask_version(kind: ToolKind, program: &Path) -> Option<FoundTool> {
    let mut command = Command::new(program);
    command.args(kind.version_arguments());
    let output = run_with_limits(command, VERSION_TIMEOUT, &|| true).ok()?;
    if output.exit_code != Some(0) {
        return None;
    }
    // older javac prints its version on stderr
    let printed = if output.stdout.trim().is_empty() {
        output.stderr
    } else {
        output.stdout
    };
    Some(FoundTool {
        kind,
        path: program.to_path_buf(),
        label: version_label(kind, &printed),
    })
}

/// every tool found on this computer, each asked in parallel so the slow
/// ones (java) don't add up.
pub fn find_tools(folders: &[PathBuf]) -> Vec<FoundTool> {
    let askers: Vec<_> = ToolKind::ALL
        .iter()
        .filter_map(|kind| {
            let program = find_program(kind.program_names(), folders)?;
            let kind = *kind;
            Some(std::thread::spawn(move || ask_version(kind, &program)))
        })
        .collect();
    askers
        .into_iter()
        .filter_map(|asker| asker.join().ok().flatten())
        .collect()
}

/// the folders of this computer, from its environment.
pub fn this_computers_folders() -> Vec<PathBuf> {
    let path_variable = std::env::var("PATH").ok();
    let local_app_data = std::env::var_os("LOCALAPPDATA").map(PathBuf::from);
    search_folders(path_variable.as_deref(), local_app_data.as_deref())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_lines_become_short_labels() {
        assert_eq!(version_label(ToolKind::Gcc, "16.2.0\n"), "gcc 16.2");
        assert_eq!(
            version_label(ToolKind::Python, "Python 3.12.1\n"),
            "Python 3.12.1"
        );
        assert_eq!(
            version_label(ToolKind::Javac, "javac 21.0.2\n"),
            "javac 21.0.2"
        );
        assert_eq!(
            version_label(ToolKind::Node, "v22.11.0\n"),
            "Node.js 22.11.0"
        );
        assert_eq!(
            version_label(
                ToolKind::Clang,
                "clang version 18.1.8 (Fedora)\nTarget: x86_64"
            ),
            "clang 18.1"
        );
        assert_eq!(version_label(ToolKind::VsCode, ""), "Visual Studio Code");
    }

    #[test]
    fn per_user_folders_are_searched_after_path() {
        let folders = search_folders(None, Some(Path::new("C:/Users/sam/AppData/Local")));
        assert!(folders
            .iter()
            .any(|folder| folder.ends_with("Programs/mingw64/bin")));
        assert!(folders
            .iter()
            .any(|folder| folder.ends_with("Programs/Microsoft VS Code/bin")));
    }

    #[test]
    fn a_program_is_found_in_the_first_folder_that_has_it() {
        let root = tempfile::tempdir().unwrap();
        let first = root.path().join("first");
        let second = root.path().join("second");
        std::fs::create_dir_all(&first).unwrap();
        std::fs::create_dir_all(&second).unwrap();
        let file_name = if cfg!(windows) { "gcc.exe" } else { "gcc" };
        std::fs::write(second.join(file_name), b"").unwrap();
        let found = find_program(&["gcc"], &[first, second.clone()]).unwrap();
        assert_eq!(found, second.join(file_name));
        assert!(find_program(&["clang"], &[second]).is_none());
    }

    #[test]
    fn the_store_python_placeholder_never_counts() {
        let stub = Path::new(r"C:\Users\sam\AppData\Local\Microsoft\WindowsApps\python3.exe");
        assert!(is_store_placeholder(stub));
    }
}

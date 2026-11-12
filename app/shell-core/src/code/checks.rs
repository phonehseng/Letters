//! compiler-backed syntax checks: "is this C file well-formed?" asked of
//! a compiler that is already installed, without building or running.
//!
//! the text is copied into a fresh folder in the temp directory and the
//! checker reads that copy. every command asks only for a syntax check
//! (gcc -fsyntax-only, python's ast.parse, javac -proc:none into the temp
//! folder, node --check), so the person's program never runs and nothing
//! is written beside their file. the raw output goes back to the ui, which
//! reads it into problems (lib/code/diagnostics.ts).

use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use serde::{Deserialize, Serialize};

use super::runner::{run_with_limits, RunOutput};
use super::tools::{FoundTool, ToolKind};

/// a syntax check of one file takes well under a second; a check still
/// running after this long is stuck (a huge include tree, a slow disk)
pub const CHECK_TIMEOUT: Duration = Duration::from_secs(15);

/// python's own parser, asked through its standard library. it parses and
/// stops: `ast.parse` builds a tree and never runs the code. warnings the
/// parser raises (an invalid escape) are printed in the same form
const PYTHON_SYNTAX_SCRIPT: &str = r#"import ast, sys, warnings
path = sys.argv[1]
with warnings.catch_warnings(record=True) as caught:
    warnings.simplefilter("always")
    try:
        ast.parse(open(path, "rb").read(), path)
    except SyntaxError as problem:
        print("%s:%d:%d: error: %s" % (path, problem.lineno or 1, problem.offset or 1, problem.msg))
    except ValueError as problem:
        print("%s:1:1: error: %s" % (path, problem))
    for warning in caught:
        print("%s:%d:1: warning: %s" % (path, warning.lineno or 1, warning.message))
"#;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CheckLanguage {
    C,
    Cpp,
    Python,
    Java,
    JavaScript,
}

impl CheckLanguage {
    /// the checkers that can check this language, best first.
    pub fn checkers(self) -> &'static [ToolKind] {
        match self {
            CheckLanguage::C => &[ToolKind::Gcc, ToolKind::Clang],
            CheckLanguage::Cpp => &[ToolKind::Gxx, ToolKind::ClangXx],
            CheckLanguage::Python => &[ToolKind::Python],
            CheckLanguage::Java => &[ToolKind::Javac],
            CheckLanguage::JavaScript => &[ToolKind::Node],
        }
    }

    fn default_file_name(self) -> &'static str {
        match self {
            CheckLanguage::C => "untitled.c",
            CheckLanguage::Cpp => "untitled.cpp",
            CheckLanguage::Python => "untitled.py",
            CheckLanguage::Java => "Main.java",
            CheckLanguage::JavaScript => "untitled.js",
        }
    }

    /// what the ui says when no checker is installed.
    pub fn missing_sentence(self) -> &'static str {
        match self {
            CheckLanguage::C | CheckLanguage::Cpp => {
                "Basic check only: no C compiler (gcc or clang) found on this computer."
            }
            CheckLanguage::Python => "Basic check only: Python isn’t installed on this computer.",
            CheckLanguage::Java => {
                "Basic check only: no Java compiler (javac) found on this computer."
            }
            CheckLanguage::JavaScript => {
                "Checked inside Letters. Node.js isn’t installed, so this is the only check."
            }
        }
    }
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CheckRequest {
    pub language: CheckLanguage,
    pub text: String,
    /// the file being edited, if it has one: its name is reused for the
    /// copy (javac needs the class name) and its folder is where includes
    /// and other classes are read from, in place
    pub path: Option<String>,
}

/// what the ui gets back: the checker's words, and which checker said them.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CheckOutcome {
    /// "gcc 16.2", or None when no checker is installed
    pub checker: Option<String>,
    /// gcc, python, javac or node: which reader the ui should use
    pub output_format: Option<ToolKind>,
    /// stderr then stdout, as printed
    pub output: String,
    /// the name of the checked copy, as it appears in the output
    pub file_name: String,
    pub timed_out: bool,
    pub cancelled: bool,
    /// a sentence for the status line when there was no checker
    pub note: Option<String>,
}

/// the arguments for a syntax check of `file`, and nothing else. kept apart
/// from running so tests can see exactly which flags are used.
pub fn check_arguments(
    kind: ToolKind,
    file: &Path,
    temp_folder: &Path,
    source_folder: Option<&Path>,
) -> Vec<OsString> {
    let mut arguments: Vec<OsString> = Vec::new();
    match kind {
        ToolKind::Gcc | ToolKind::Gxx | ToolKind::Clang | ToolKind::ClangXx => {
            arguments.extend(c_family_flags(kind).iter().map(OsString::from));
            if let Some(folder) = source_folder {
                // quoted includes resolve beside the original file, read in place
                arguments.push("-I".into());
                arguments.push(folder.into());
            }
            arguments.push(file.into());
        }
        ToolKind::Python => {
            // -I: ignore PYTHON* variables and the current folder, -S: no site packages
            arguments.extend(["-I", "-S", "-c", PYTHON_SYNTAX_SCRIPT].map(OsString::from));
            arguments.push(file.into());
        }
        ToolKind::Javac => {
            // -proc:none: annotation processors are code too, so none run.
            // -d: the class files go to the temp folder, never beside the source
            arguments
                .extend(["-proc:none", "-implicit:none", "-Xlint:all", "-d"].map(OsString::from));
            arguments.push(temp_folder.into());
            if let Some(folder) = source_folder {
                arguments.push("-sourcepath".into());
                arguments.push(folder.into());
            }
            arguments.push(file.into());
        }
        ToolKind::Node => {
            arguments.push("--check".into());
            arguments.push(file.into());
        }
        ToolKind::VsCode => {}
    }
    arguments
}

fn c_family_flags(kind: ToolKind) -> &'static [&'static str] {
    let is_cpp = matches!(kind, ToolKind::Gxx | ToolKind::ClangXx);
    let is_gcc = matches!(kind, ToolKind::Gcc | ToolKind::Gxx);
    // -fsyntax-only: parse and type-check, write nothing, link nothing.
    // -Wall: the warnings a teacher would want a beginner to see
    match (is_gcc, is_cpp) {
        (true, false) => &[
            "-fsyntax-only",
            "-Wall",
            "-x",
            "c",
            "-fdiagnostics-plain-output",
            "-fmax-errors=50",
        ],
        (true, true) => &[
            "-fsyntax-only",
            "-Wall",
            "-x",
            "c++",
            "-fdiagnostics-plain-output",
            "-fmax-errors=50",
        ],
        (false, false) => &[
            "-fsyntax-only",
            "-Wall",
            "-x",
            "c",
            "-fno-color-diagnostics",
            "-fno-caret-diagnostics",
        ],
        (false, true) => &[
            "-fsyntax-only",
            "-Wall",
            "-x",
            "c++",
            "-fno-color-diagnostics",
            "-fno-caret-diagnostics",
        ],
    }
}

/// the first installed checker for the language.
pub fn checker_for(language: CheckLanguage, tools: &[FoundTool]) -> Option<&FoundTool> {
    language
        .checkers()
        .iter()
        .find_map(|kind| tools.iter().find(|tool| tool.kind == *kind))
}

/// checks `request` with the best installed checker. `still_wanted` stops
/// it early when a newer check has started.
pub fn run_check(
    request: &CheckRequest,
    tools: &[FoundTool],
    still_wanted: &dyn Fn() -> bool,
) -> Result<CheckOutcome, String> {
    let file_name = copy_name(request);
    let Some(tool) = checker_for(request.language, tools) else {
        return Ok(CheckOutcome {
            checker: None,
            output_format: None,
            output: String::new(),
            file_name,
            timed_out: false,
            cancelled: false,
            note: Some(request.language.missing_sentence().to_owned()),
        });
    };
    let folder = TempCheckFolder::create()?;
    let copy = folder.path.join(&file_name);
    fs::write(&copy, &request.text).map_err(|error| error.to_string())?;
    let source_folder = request
        .path
        .as_deref()
        .and_then(|path| Path::new(path).parent());
    let mut command = Command::new(&tool.path);
    command
        .args(check_arguments(
            tool.kind,
            &copy,
            &folder.path,
            source_folder,
        ))
        .current_dir(&folder.path);
    let output = run_with_limits(command, CHECK_TIMEOUT, still_wanted)
        .map_err(|error| format!("{} couldn’t be started. ({error})", tool.label))?;
    Ok(outcome_from(tool, file_name, output))
}

fn outcome_from(tool: &FoundTool, file_name: String, output: RunOutput) -> CheckOutcome {
    let note = output
        .timed_out
        .then(|| format!("{} took too long and was stopped.", tool.label));
    CheckOutcome {
        checker: Some(tool.label.clone()),
        output_format: Some(tool.kind),
        output: format!("{}{}", output.stderr, output.stdout),
        file_name,
        timed_out: output.timed_out,
        cancelled: output.cancelled,
        note,
    }
}

/// the file's own name when it has one and it is safe to reuse, else a default.
fn copy_name(request: &CheckRequest) -> String {
    let own_name = request
        .path
        .as_deref()
        .and_then(|path| Path::new(path).file_name())
        .map(|name| name.to_string_lossy().into_owned())
        .filter(|name| is_plain_file_name(name));
    own_name.unwrap_or_else(|| request.language.default_file_name().to_owned())
}

// a name with only letters, digits, dots, dashes and underscores can't be
// read as an option or a path by any checker
fn is_plain_file_name(name: &str) -> bool {
    !name.starts_with(['-', '.'])
        && name
            .chars()
            .all(|c| c.is_alphanumeric() || matches!(c, '.' | '-' | '_'))
}

/// a folder of its own for one check, removed when the check is done.
struct TempCheckFolder {
    path: PathBuf,
}

static NEXT_FOLDER: AtomicU64 = AtomicU64::new(0);

impl TempCheckFolder {
    fn create() -> Result<TempCheckFolder, String> {
        let number = NEXT_FOLDER.fetch_add(1, Ordering::SeqCst);
        let path = std::env::temp_dir()
            .join("letters-code-check")
            .join(format!("{}-{number}", std::process::id()));
        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(&path).map_err(|error| error.to_string())?;
        Ok(TempCheckFolder { path })
    }
}

impl Drop for TempCheckFolder {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

#[cfg(test)]
#[path = "checks_tests.rs"]
mod tests;

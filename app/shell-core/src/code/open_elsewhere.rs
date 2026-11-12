//! "Open in VS Code" and "Show in File Explorer" (Finder on a mac).
//!
//! both hand the file to another program and return at once; Letters
//! doesn't wait for or watch that program. vs code is only used when it is
//! already installed (see tools.rs); nothing here installs it.

use std::ffi::OsString;
use std::path::Path;
use std::process::{Command, Stdio};

/// the arguments that open `path` in vs code with the cursor on `line`
/// and `column` (both counted from 1). `--goto` takes "file:line:column".
pub fn vscode_arguments(path: &Path, line: u32, column: u32) -> Vec<OsString> {
    let mut place = path.as_os_str().to_owned();
    place.push(format!(":{}:{}", line.max(1), column.max(1)));
    vec![OsString::from("--goto"), place]
}

pub fn open_in_vscode(program: &Path, path: &Path, line: u32, column: u32) -> Result<(), String> {
    let mut command = Command::new(program);
    command.args(vscode_arguments(path, line, column));
    start_detached(command)
        .map_err(|error| format!("Visual Studio Code couldn’t be opened. ({error})"))
}

/// the program and arguments that show `path` selected in its folder.
pub fn reveal_command(path: &Path) -> (OsString, Vec<OsString>) {
    if cfg!(windows) {
        // explorer reads "/select," and the path as one argument
        let mut select = OsString::from("/select,");
        select.push(path.as_os_str());
        (OsString::from("explorer"), vec![select])
    } else if cfg!(target_os = "macos") {
        (OsString::from("open"), vec!["-R".into(), path.into()])
    } else {
        let folder = path.parent().unwrap_or(path);
        (OsString::from("xdg-open"), vec![folder.into()])
    }
}

pub fn reveal_in_folder(path: &Path) -> Result<(), String> {
    let (program, arguments) = reveal_command(path);
    let mut command = Command::new(program);
    command.args(arguments);
    start_detached(command).map_err(|error| format!("The folder couldn’t be shown. ({error})"))
}

// the other program keeps running after this returns; its output is not read.
// vs code's "code" is a .cmd script on windows, which would flash a console
fn start_detached(mut command: Command) -> std::io::Result<()> {
    super::runner::hide_console_window(&mut command);
    command
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    command.spawn().map(|_child| ())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vscode_opens_the_file_at_the_line_and_column() {
        let arguments = vscode_arguments(Path::new("/home/sam/hello.c"), 12, 5);
        assert_eq!(arguments[0], "--goto");
        assert_eq!(arguments[1], "/home/sam/hello.c:12:5");
    }

    #[test]
    fn line_zero_becomes_the_first_line() {
        let arguments = vscode_arguments(Path::new("a.py"), 0, 0);
        assert_eq!(arguments[1], "a.py:1:1");
    }

    #[test]
    fn reveal_selects_the_file_in_its_folder() {
        let (program, arguments) = reveal_command(Path::new("/home/sam/hello.c"));
        assert!(!program.is_empty());
        assert!(!arguments.is_empty());
    }
}

//! "Move to Recycle Bin": the system's own recycle bin (the Trash on a
//! mac), so a file can always be put back from there. nothing in Letters
//! deletes a person's document for good.
//!
//! it asks the system through its own tools (windows' recycle bin through
//! the visual basic file system routines that every windows has, the
//! finder on a mac), so no extra library is needed.

use std::path::Path;
use std::process::Command;

use super::files::OrganiseError;

/// moves a file to the recycle bin.
pub fn to_recycle_bin(path: &Path) -> Result<(), OrganiseError> {
    if !path.exists() {
        return Ok(());
    }
    let status = recycle_command(path).status().map_err(|e| {
        OrganiseError::Message(format!("The recycle bin couldn\u{2019}t be reached ({e})."))
    })?;
    if status.success() && !path.exists() {
        Ok(())
    } else {
        let name = path.file_name().unwrap_or_default().to_string_lossy();
        Err(OrganiseError::Message(format!(
            "\u{201C}{name}\u{201D} couldn\u{2019}t be moved to the Recycle Bin. It may be open in another program."
        )))
    }
}

#[cfg(windows)]
fn recycle_command(path: &Path) -> Command {
    use std::os::windows::process::CommandExt;
    // the path is passed through an environment variable, never pasted into
    // the script, so no file name can be read as a command
    let script = "Add-Type -AssemblyName Microsoft.VisualBasic; \
        [Microsoft.VisualBasic.FileIO.FileSystem]::DeleteFile($env:LETTERS_RECYCLE, 'OnlyErrorDialogs', 'SendToRecycleBin')";
    let mut command = Command::new("powershell");
    command
        .args(["-NoProfile", "-NonInteractive", "-Command", script])
        .env("LETTERS_RECYCLE", path)
        // no console window flashes up
        .creation_flags(0x0800_0000);
    command
}

#[cfg(not(windows))]
fn recycle_command(path: &Path) -> Command {
    let mut command = Command::new("osascript");
    command
        .args([
            "-e",
            "on run argv",
            "-e",
            "tell application \"Finder\" to delete (POSIX file (item 1 of argv) as alias)",
            "-e",
            "end run",
        ])
        .arg(path);
    command
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_missing_file_needs_nothing() {
        assert!(to_recycle_bin(Path::new("C:/surely/not/here.pdf")).is_ok());
    }
}

//! reading the command line the app was started with.

use std::path::PathBuf;

/// the file to open, if the app was started with one (for example by
/// double clicking a pdf). the first argument is the program itself.
pub fn file_argument(arguments: &[String]) -> Option<PathBuf> {
    arguments
        .iter()
        .skip(1)
        .find(|argument| !argument.starts_with('-'))
        .map(PathBuf::from)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn arguments(list: &[&str]) -> Vec<String> {
        list.iter().map(|text| text.to_string()).collect()
    }

    #[test]
    fn no_file_when_started_plainly() {
        assert_eq!(file_argument(&arguments(&["letters.exe"])), None);
    }

    #[test]
    fn finds_the_file_after_the_program_name() {
        let found = file_argument(&arguments(&["letters.exe", "C:\\notes\\essay.pdf"]));
        assert_eq!(found, Some(PathBuf::from("C:\\notes\\essay.pdf")));
    }

    #[test]
    fn skips_flags() {
        let found = file_argument(&arguments(&["letters", "--verbose", "essay.pdf"]));
        assert_eq!(found, Some(PathBuf::from("essay.pdf")));
    }
}

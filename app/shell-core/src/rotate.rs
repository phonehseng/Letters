//! turning pages: the reference example of an edit.
//!
//! every later edit (page tools, annotations, redaction, forms) has this
//! same shape, so copy it:
//!
//! 1. a plain function that takes `&mut lopdf::Document` plus the edit's
//!    own settings, and changes the document in memory.
//! 2. it checks everything it needs before changing anything, and
//!    returns its own error type (with a `Display` message written for
//!    the person) when it refuses.
//! 3. it knows nothing about files, undo or windows. the shell runs it
//!    through `EditSession::apply` (see session.rs), which takes care of
//!    the working copy, undo, and showing the result; a failed edit
//!    leaves the document as it was however far the function got.
//! 4. it has unit tests here, on small pdf files.
//!
//! the shell side is the `rotate_pages` command in app/shell/src/commands.rs,
//! which is one line: `state.edit(id, |document| rotate_pages(document, ...))`.

use std::collections::BTreeSet;
use std::fmt;

use lopdf::{Document, Object, ObjectId};

/// a turn of a quarter, half or three quarters, either way.
const QUARTER_TURN: i32 = 90;

/// how far up the page tree to look for an inherited rotation. real
/// files are a few levels deep; the limit only stops a looping tree.
const MAX_TREE_DEPTH: usize = 64;

/// why pages could not be turned.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RotateError {
    NotAQuarterTurn(i32),
    NoPagesChosen,
    /// counted from 0, as the ui sends it
    UnknownPage(u32),
    Damaged(String),
}

impl fmt::Display for RotateError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotAQuarterTurn(degrees) => write!(
                f,
                "Pages can only be turned by quarter turns, not by {degrees}°."
            ),
            Self::NoPagesChosen => write!(f, "Choose a page to rotate first."),
            Self::UnknownPage(page) => {
                write!(f, "Page {} doesn’t exist in this document.", page + 1)
            }
            Self::Damaged(detail) => write!(
                f,
                "This page can’t be rotated because part of the PDF can’t be read. ({detail})"
            ),
        }
    }
}

impl std::error::Error for RotateError {}

/// turns each of `pages` (counted from 0) by `degrees` clockwise.
/// negative degrees turn anticlockwise.
///
/// a page's turn is stored in its /Rotate entry. a page without one
/// inherits it from the page tree above, so the turn is added to the
/// inherited value and written on the page itself, which then wins.
/// a page named twice is turned once.
pub fn rotate_pages(
    document: &mut Document,
    pages: &[u32],
    degrees: i32,
) -> Result<(), RotateError> {
    if degrees % QUARTER_TURN != 0 {
        return Err(RotateError::NotAQuarterTurn(degrees));
    }
    if pages.is_empty() {
        return Err(RotateError::NoPagesChosen);
    }
    // every page is found before any is changed, so a bad page number
    // refuses the whole edit instead of turning half the pages
    let targets = find_pages(document, pages)?;
    for page in targets {
        let turned = normalise(rotation_of(document, page) + i64::from(degrees));
        document
            .get_dictionary_mut(page)
            .map_err(|error| RotateError::Damaged(error.to_string()))?
            .set("Rotate", turned);
    }
    Ok(())
}

/// the page objects for the given page numbers, without repeats.
fn find_pages(document: &Document, pages: &[u32]) -> Result<BTreeSet<ObjectId>, RotateError> {
    // lopdf counts pages from 1
    let all_pages = document.get_pages();
    pages
        .iter()
        .map(|&page| {
            all_pages
                .get(&(page + 1))
                .copied()
                .ok_or(RotateError::UnknownPage(page))
        })
        .collect()
}

/// the turn a page has now: its own /Rotate, or the nearest one above it.
pub fn rotation_of(document: &Document, page: ObjectId) -> i64 {
    let mut node = Some(page);
    for _ in 0..MAX_TREE_DEPTH {
        let Some(id) = node else { break };
        let Ok(dictionary) = document.get_dictionary(id) else {
            break;
        };
        if let Some(rotate) = read_rotate(document, dictionary.get(b"Rotate").ok()) {
            return rotate;
        }
        node = dictionary
            .get(b"Parent")
            .and_then(Object::as_reference)
            .ok();
    }
    0
}

/// a /Rotate value may be stored directly or as a reference to a number.
fn read_rotate(document: &Document, value: Option<&Object>) -> Option<i64> {
    let (_, value) = document.dereference(value?).ok()?;
    match value {
        Object::Integer(number) => Some(*number),
        // a few writers store whole numbers as reals
        Object::Real(number) => Some(number.round() as i64),
        _ => None,
    }
}

/// brings a turn into 0, 90, 180 or 270, the values pdf allows.
fn normalise(degrees: i64) -> i64 {
    degrees.rem_euclid(360)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_pdf::{page_rotation, temp_dir, write_pdf, write_pdf_with_inherited_rotation};

    fn load(path: &std::path::Path) -> Document {
        Document::load(path).unwrap()
    }

    /// turns pages in a file and saves it, returning the result.
    fn rotate_file(path: &std::path::Path, pages: &[u32], degrees: i32) -> Result<(), RotateError> {
        let mut document = load(path);
        rotate_pages(&mut document, pages, degrees)?;
        document.save(path).unwrap();
        Ok(())
    }

    #[test]
    fn turns_only_the_chosen_pages() {
        let path = temp_dir("rotate-chosen").join("three.pdf");
        write_pdf(&path, 3);

        rotate_file(&path, &[1], 90).unwrap();

        assert_eq!(page_rotation(&path, 1), 0);
        assert_eq!(page_rotation(&path, 2), 90);
        assert_eq!(page_rotation(&path, 3), 0);
    }

    #[test]
    fn turning_left_from_upright_gives_270() {
        let path = temp_dir("rotate-left").join("one.pdf");
        write_pdf(&path, 1);

        rotate_file(&path, &[0], -90).unwrap();

        assert_eq!(page_rotation(&path, 1), 270);
    }

    #[test]
    fn four_quarter_turns_come_back_upright() {
        let path = temp_dir("rotate-full").join("one.pdf");
        write_pdf(&path, 1);

        for _ in 0..4 {
            rotate_file(&path, &[0], 90).unwrap();
        }

        assert_eq!(page_rotation(&path, 1), 0);
    }

    #[test]
    fn adds_to_a_rotation_inherited_from_the_page_tree() {
        let path = temp_dir("rotate-inherited").join("two.pdf");
        write_pdf_with_inherited_rotation(&path, 2, Some(90));

        rotate_file(&path, &[0], 90).unwrap();

        assert_eq!(page_rotation(&path, 1), 180);
        // the other page still inherits the tree's quarter turn
        assert_eq!(page_rotation(&path, 2), 90);
    }

    #[test]
    fn a_page_named_twice_is_turned_once() {
        let path = temp_dir("rotate-twice").join("one.pdf");
        write_pdf(&path, 1);

        rotate_file(&path, &[0, 0], 90).unwrap();

        assert_eq!(page_rotation(&path, 1), 90);
    }

    #[test]
    fn refuses_turns_that_are_not_quarter_turns() {
        let path = temp_dir("rotate-odd").join("one.pdf");
        write_pdf(&path, 1);
        let mut document = load(&path);

        let result = rotate_pages(&mut document, &[0], 45);

        assert_eq!(result, Err(RotateError::NotAQuarterTurn(45)));
    }

    #[test]
    fn refuses_a_missing_page_without_turning_any() {
        let path = temp_dir("rotate-missing").join("two.pdf");
        write_pdf(&path, 2);

        let result = rotate_file(&path, &[0, 5], 90);

        assert_eq!(result, Err(RotateError::UnknownPage(5)));
        assert_eq!(page_rotation(&path, 1), 0);
        assert_eq!(
            RotateError::UnknownPage(5).to_string(),
            "Page 6 doesn’t exist in this document."
        );
    }

    #[test]
    fn refuses_an_empty_list_of_pages() {
        let path = temp_dir("rotate-none").join("one.pdf");
        write_pdf(&path, 1);
        let mut document = load(&path);

        assert_eq!(
            rotate_pages(&mut document, &[], 90),
            Err(RotateError::NoPagesChosen)
        );
    }

    #[test]
    fn normalises_any_quarter_turn() {
        assert_eq!(normalise(-90), 270);
        assert_eq!(normalise(450), 90);
        assert_eq!(normalise(-720), 0);
    }
}

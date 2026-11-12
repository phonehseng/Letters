use thiserror::Error;

/// everything that can go wrong when changing pages.
///
/// the ui shows these messages as they are, so they are written for the
/// person using the app: a plain sentence, no blame, and where possible
/// what to do next. page numbers in messages count from 1, the way people
/// count, even though the api counts from 0.
#[derive(Debug, Error, Clone, PartialEq)]
pub enum PagesError {
    #[error("This PDF is locked with a password. Unlock it first, then try again.")]
    Locked,

    #[error("The list of pages in this PDF is damaged, so its pages can’t be changed. ({0})")]
    Damaged(String),

    /// holds the 1-based page number, ready to show
    #[error("Page {0} doesn’t exist in this document.")]
    NoSuchPage(u32),

    /// holds the 1-based page number, ready to show
    #[error("Page {0} was chosen more than once.")]
    PageChosenTwice(u32),

    #[error("Choose at least one page first.")]
    NoPagesChosen,

    #[error("A PDF needs at least one page, so not every page can be deleted.")]
    CannotDeleteEveryPage,

    #[error("The new page order must list every page exactly once.")]
    NotAPermutation,

    #[error("Pages can only be added before, between or after the existing pages.")]
    NoSuchPosition,

    #[error("A page must be between 3 and 14,400 points (about 0.04 to 200 inches) on each side.")]
    PageSizeOutOfRange,

    #[error("Add between 1 and 1,000 blank pages at a time.")]
    BlankPageCountOutOfRange,

    #[error("Choose at least one document to combine.")]
    NothingToMerge,

    #[error("Each document being combined needs a name for its bookmark.")]
    MergeNamesMissing,

    /// holds the 1-based start and end pages
    #[error(
        "Pages {0} to {1} isn’t a range that can be split off. Start with the lower page number."
    )]
    BackwardsRange(u32, u32),

    #[error("Each part needs at least one page.")]
    PartSizeZero,

    #[error("This PDF has no bookmarks at the top level to split it by.")]
    NoBookmarks,

    #[error("Choose a file size limit bigger than zero.")]
    SizeLimitZero,

    /// holds the 1-based page number
    #[error("The crop area doesn’t fit on page {0}. Draw it inside the page.")]
    CropOutsidePage(u32),

    #[error("The crop area is too small. Make it at least 3 points wide and tall.")]
    CropTooSmall,

    /// a character the standard pdf fonts can't draw
    #[error("The character “{0}” can’t be used in a stamp. Stamps can use letters, numbers and symbols from Western European languages.")]
    UnsupportedCharacter(char),

    #[error("This picture can’t be used as a watermark. Use a PNG or JPEG file. ({0})")]
    UnsupportedImage(String),

    /// a setting outside what makes sense; the text is the whole sentence
    #[error("{0}")]
    InvalidSetting(String),

    #[error("Something went wrong while changing the pages. Nothing was changed. ({0})")]
    Internal(String),
}

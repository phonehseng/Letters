//! the errors of the font library, each a sentence the person can act on.

use thiserror::Error;

/// everything that can go wrong with fonts.
///
/// the ui shows these messages as they are, so they are written for the
/// person using the app: a plain sentence, no blame, and where possible
/// what to do next.
#[derive(Debug, Error, Clone, PartialEq)]
pub enum FontError {
    #[error(
        "“{0}” isn’t a font file Letters can use. Choose a .ttf, .otf, .ttc, .woff or .woff2 file."
    )]
    NotAFont(String),

    #[error("“{0}” is damaged and can’t be used. Download the font again from where you got it.")]
    Damaged(String),

    #[error(
        "“{name}” is too large to be a font ({megabytes} MB). Fonts are rarely over {limit} MB."
    )]
    TooLarge {
        name: String,
        megabytes: u64,
        limit: u64,
    },

    #[error("“{0}” can’t be read. Check that the file still exists and try again.")]
    CannotRead(String),

    #[error("The font can’t be saved in Letters’ folder. Check that the disk isn’t full and try again. ({0})")]
    CannotSave(String),

    #[error("This font is no longer available.")]
    UnknownFont,

    #[error(
        "Only fonts you added can be removed. Fonts on this computer are managed by the system."
    )]
    NotRemovable,

    #[error("“{0}” isn’t in the font library.")]
    NotInCatalogue(String),

    #[error("The download didn’t finish. Check the internet connection and try again; it continues where it stopped. ({0})")]
    DownloadFailed(String),

    #[error("The downloaded file for “{0}” isn’t the one the library expects, so it was deleted. Try again later.")]
    HashMismatch(String),

    #[error("The download was cancelled.")]
    Cancelled,

    #[error("This font doesn’t allow embedding, so it can’t be saved inside documents. Choose a similar font that does.")]
    EmbeddingNotAllowed,

    #[error("Only fonts with TrueType outlines (most .ttf files) can be made smaller for embedding. The whole font is embedded instead.")]
    CannotSubset,
}

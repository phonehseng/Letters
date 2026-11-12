//! removes content for good, scrubs hidden data, and locks files with a password.
//!
//! everything here is a plain operation on a `lopdf::Document` with no
//! window or file system involved, so it can all be tested on its own:
//!
//! - [`redact`] takes out the text, images, vector paths, annotations and
//!   form values under some boxes and draws the boxes. [`preview`] says
//!   what it would take out without changing anything.
//! - [`find_text_areas`] finds a phrase or a kind of text (email
//!   addresses, phone numbers, long numbers) and returns boxes for it.
//! - [`scrub`] removes hidden information; [`scrub_preview`] lists it.
//! - [`encrypt`], [`open`] and [`unlock`] deal with passwords.
//! - [`save_clean`] writes the result. always use it after redacting or
//!   scrubbing: an incremental save would keep the old content in the file.
//!
//! positions are in points from the top left corner of the page as it is
//! shown (after its crop box and rotation), with y growing downward, and
//! pages are numbered from 0. see README.md for what is guaranteed and
//! every known limit.

mod cleanup;
mod content;
mod error;
mod find;
mod fonts;
mod geometry;
mod lock;
mod objects;
mod page;
mod redact;
mod scrub;
mod text_layout;
mod warnings;

pub use cleanup::save_clean;
pub use error::RedactError;
pub use find::{find_text_areas, FindOptions, FoundText, TextQuery, MAX_FOUND};
pub use lock::{
    encrypt, has_restrictions, lock_info, needs_password_to_open, open, unlock, unlock_with_info,
    LockInfo, LockOptions, PasswordMatch, PermissionOptions, PrintPermission, Unlocked,
};
pub use page::PageRect;
pub use redact::{
    preview, preview_with_options, redact, AreaPreview, PageReport, RedactOptions, RedactionArea,
    RedactionPreview, RedactionReport, Rgb,
};
pub use scrub::{scrub, scrub_preview, ScrubFinding, ScrubItem, ScrubOptions, ScrubReport};

/// re-exported so callers use the same lopdf as this crate.
pub use lopdf;

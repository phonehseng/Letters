//! comparing two versions of a document (the Compare screen).
//!
//! the engine is core/compare. what the shell adds:
//! - `job.rs`: one comparison on its own thread, with progress, cancel,
//!   the overlay pictures and the report, and closing everything after
//! - `jobs.rs`: the comparisons of every window, and their folders
//! - `renderer.rs`: whole pages and parts of pages drawn from the worker's
//!   tiles, for the engine's picture comparison (and the accessibility
//!   checker's contrast measuring)

mod job;
mod jobs;
mod renderer;
#[cfg(test)]
pub(crate) mod test_files;
#[cfg(test)]
mod tests;

use std::fs;
use std::path::{Path, PathBuf};

pub use job::{CompareJob, JobStatus, OpenedSide, PairPictures, ReportFormat, SideSpec};
pub use jobs::CompareJobs;
pub use renderer::{decode_png, encode_png, render_page, safe_scale, scale_in_thousandths};

/// copies an open document's working copy into the job's folder, so the
/// comparison reads it as it is now, even if it is edited meanwhile.
pub fn copy_for_comparison(working_copy: &Path, folder: &Path) -> Result<PathBuf, String> {
    let copy = folder.join("open-document.pdf");
    fs::copy(working_copy, &copy).map_err(|error| {
        format!("The open document couldn’t be copied for the comparison. Try again. ({error})")
    })?;
    Ok(copy)
}

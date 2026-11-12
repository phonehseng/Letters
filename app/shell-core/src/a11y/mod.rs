//! the accessibility checker and fix wizard (the Accessibility tab).
//!
//! the engine is core/a11y. what the shell adds:
//! - `region.rs`: parts of pages drawn by the app's own pdf engine, so the
//!   checker can measure contrast, and pictures of a figure or a table for
//!   the wizard
//! - `fixes.rs`: the wizard's requests and running each with the engine,
//!   inside one edit, so every fix is one undo step
//! - `blocks.rs`: where each tagged block of a page is, for the reading
//!   order step

mod blocks;
mod fixes;
mod region;
#[cfg(test)]
mod tests;

pub use blocks::{reading_blocks, ReadingBlock};
pub use fixes::{apply_fix, FixOutcome, FixRequest, HeadingLevel};
pub use region::{crop, pixel_area, region_png, PageRegions};

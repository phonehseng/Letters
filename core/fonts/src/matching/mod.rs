//! matching a document's font: outlines as geometry, how far apart two
//! outlines are, the measured style of a font, which typeface a font
//! really is, and fitting a near design to it.
//!
//! | module | job |
//! |---|---|
//! | `shape` | outlines in em units: contours of on and off points |
//! | `distance` | resampling outlines and measuring how far apart they are |
//! | `glyphs` | reading characters' glyphs and metrics from a font |
//! | `style` | stem weight, heights, slant, contrast, serifs, spacing |
//! | `offset` | making strokes heavier or lighter |
//! | `donors` | the fonts that may lend glyphs, and their licences |
//! | `identify` | which font is the same design, or the closest |
//! | `fit` | bending a near design to imitate the target |
//! | `optimise` | least squares and one-dimensional searches |

pub mod distance;
pub mod donors;
pub mod fit;
pub mod glyphs;
pub mod identify;
pub mod offset;
pub mod optimise;
pub mod shape;
pub mod style;

pub use distance::{shape_distance, Samples, ShapeDistance};
pub use donors::{DonorFile, DonorPool};
pub use fit::{FitQuality, FittedDonor, SpacingModel, Transform};
pub use glyphs::{FontMetrics, GlyphData, GlyphSet};
pub use identify::{similarity_percent, Candidate, IdentifyOptions};
pub use shape::{Bounds, Contour, Node, Shape, ShapeBuilder};
pub use style::{measure_style, StyleFit};

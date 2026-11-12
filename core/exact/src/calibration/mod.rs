//! the measured facts the recipe needs, per word build (recipe 9): built in
//! for the build they were measured on, cached in a folder the caller
//! gives, and measured by probe exports for unseen page sizes and builds.

pub mod model;
pub mod probe;

pub use model::{Axis, WordModel, MEASURED_BUILD};
pub use probe::{ensure_measured, measure_baselines, CalibrationReport};

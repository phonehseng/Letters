//! what was measured about microsoft word's layout and pdf writer.
//!
//! nothing here starts word: these are the pure rules of recipe sections 4
//! and 6, written so they can be unit tested. `bridge` drives word, and
//! `calibration` measures the per-build facts these rules take as input.

pub mod sizes;
pub mod spacing;
pub mod writer_model;

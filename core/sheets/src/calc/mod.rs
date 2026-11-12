//! the calculation engine: evaluating formulas with excel's rules, the
//! function library, dependency tracking for incremental recalculation,
//! spilling of dynamic arrays and circular reference detection.
//! iterative calculation is not supported; circular references are
//! reported and show 0, as excel shows them.

pub mod context;
pub mod criteria;
mod deps;
pub mod engine;
pub mod eval;
pub mod functions;
mod operators;
pub mod precedents;
mod structured;
pub mod text_numbers;
pub mod value;

pub use context::CellKey;
pub use engine::{recalculate, CalcReport, Engine, NotRecalculated};

//! the function library, one file per family. a name not found here
//! makes the formula "not recalculated": its stored value is kept and
//! the engine reports it, so a file never silently shows a wrong number.

pub mod args;
pub use arrays::sort_order;
pub use text::windows_1252;
mod arrays;
mod conditional;
mod datetime;
mod financial;
mod info;
mod logical;
mod lookup;
pub mod math;
mod references;
mod stats;
mod text;

use crate::formula::Expr;
use crate::model::ErrorValue;

use super::eval::Evaluator;
use super::value::CalcValue;

/// (family name, functions) for the support table and the catalogue
pub const FAMILIES: &[(&str, &[&str])] = &[
    ("math", math::NAMES),
    ("statistics", stats::NAMES),
    ("conditional", conditional::NAMES),
    ("logical", logical::NAMES),
    ("lookup", lookup::NAMES),
    ("references", references::NAMES),
    ("dynamic arrays", arrays::NAMES),
    ("text", text::NAMES),
    ("date and time", datetime::NAMES),
    ("financial", financial::NAMES),
    ("information", info::NAMES),
];

pub fn is_supported(name: &str) -> bool {
    FAMILIES.iter().any(|(_, names)| names.contains(&name))
}

/// the family a function belongs to
pub fn family_of(name: &str) -> Option<&'static str> {
    FAMILIES
        .iter()
        .find(|(_, names)| names.contains(&name))
        .map(|(family, _)| *family)
}

/// a family's dispatcher: none when the name is not one of its functions
type FamilyCall = fn(&Evaluator, &str, &[Expr]) -> Option<CalcValue>;

pub fn call(ev: &Evaluator, name: &str, args: &[Expr]) -> CalcValue {
    let families: [FamilyCall; 11] = [
        math::call,
        stats::call,
        conditional::call,
        logical::call,
        lookup::call,
        references::call,
        arrays::call,
        text::call,
        datetime::call,
        financial::call,
        info::call,
    ];
    for family in families {
        if let Some(result) = family(ev, name, args) {
            return result;
        }
    }
    CalcValue::error(ErrorValue::Name)
}

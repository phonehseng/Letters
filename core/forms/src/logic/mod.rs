//! forms that calculate, format and check answers, without a script engine.
//!
//! interactive pdf forms keep this logic as javascript. Letters doesn't
//! run javascript. instead it recognises the standard built-in functions
//! nearly every real form uses (AFSimple_Calculate, AFNumber_Format,
//! AFPercent_Format, AFDate_FormatEx, AFTime_Format, AFSpecial_Format,
//! AFRange_Validate...) and simple field arithmetic, turns them into the
//! declarative model below, and evaluates that model itself. scripts
//! outside that subset are left in the file untouched and reported.
//!
//! when someone sets up a calculation, format or check in Letters, it is
//! written back as those same standard functions, so pdf apps that do run
//! form scripts behave the same way.
//!
//! conditional visibility (showing or hiding fields from a script) is out
//! of scope: see `CONDITIONAL_VISIBILITY_NOTE`.

mod author;
mod engine;
pub(crate) mod eval;
pub mod format;
mod lex;
mod masks;
mod numbers;
mod parse;
mod print;
mod recognize;
pub(crate) mod script;
mod statements;

use serde::{Deserialize, Serialize};

pub use author::{set_calculation_order, set_field_logic, FieldLogicChanges};
pub(crate) use engine::fill_without_recalculating;
pub use engine::{fill_field, form_logic, recalculate, ChangedField, FillReport};
pub use format::{FieldFormat, NegativeStyle, SeparatorStyle, SpecialFormat};

/// said once per form that has scripts Letters leaves alone.
pub const CUSTOM_SCRIPT_NOTE: &str = "This form has a custom script Letters doesn’t run";

/// said when someone asks for fields that appear or hide depending on answers.
pub const CONDITIONAL_VISIBILITY_NOTE: &str = "Letters doesn’t show or hide fields depending on other answers. Forms that do this with scripts keep their scripts, but in Letters every field stays visible.";

/// the five functions of acrobat's AFSimple_Calculate.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Aggregate {
    Sum,
    Product,
    Average,
    Minimum,
    Maximum,
}

impl Aggregate {
    pub fn code(self) -> &'static str {
        match self {
            Aggregate::Sum => "SUM",
            Aggregate::Product => "PRD",
            Aggregate::Average => "AVG",
            Aggregate::Minimum => "MIN",
            Aggregate::Maximum => "MAX",
        }
    }
}

/// how a field works out its value from other fields.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum Calculation {
    /// the sum, product, average, smallest or largest of these fields
    Aggregate {
        function: Aggregate,
        fields: Vec<String>,
    },
    /// a formula in letters notation: field names, numbers, + - * / ( ),
    /// percentages (15%), comparisons, ? :, round floor ceil min max abs.
    /// names with spaces go in double quotes: "Line 1" * 2
    Formula { formula: String },
    /// a script in the safe subset that is more than one formula (it has
    /// ifs or variables). Letters runs it but can't edit it as a formula.
    Script { source: String },
}

/// a check an answer must pass.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum Validation {
    /// a number between min and max (either may be left open)
    Range {
        #[serde(default)]
        min: Option<f64>,
        #[serde(default)]
        max: Option<f64>,
    },
    /// can't be left empty (the field's required flag)
    Required,
    /// at most this many characters (the field's /MaxLen)
    MaxLength { max: u32 },
    /// a condition in letters notation that must hold, where `value` is
    /// this field's answer: `value <= budget`, `value > start_date`
    Condition {
        condition: String,
        #[serde(default)]
        message: Option<String>,
    },
    /// a check script in the safe subset that isn't a single condition
    Script { source: String },
}

/// which of a field's actions a script belongs to.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ScriptTrigger {
    Calculate,
    Format,
    Keystroke,
    Validate,
}

/// a script Letters left alone.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UnsupportedScript {
    pub trigger: ScriptTrigger,
    /// the start of the script, for people who want to look
    pub excerpt: String,
    /// why Letters doesn't run it, in a few words
    pub reason: String,
}

/// everything Letters understands about one field's logic.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FieldLogic {
    pub field: String,
    pub calculation: Option<Calculation>,
    pub format: Option<FieldFormat>,
    pub validations: Vec<Validation>,
    pub unsupported: Vec<UnsupportedScript>,
}

/// a problem with the form's calculations as a whole.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum CalculationIssue {
    /// these fields work each other out in a circle, so none of them is
    /// calculated
    Cycle {
        fields: Vec<String>,
        message: String,
    },
    /// a calculation reads a field the form doesn't have
    UnknownField {
        field: String,
        missing: String,
        message: String,
    },
    /// a calculation reads a field that is only worked out later in the
    /// order, so it uses the older value (other pdf apps do the same)
    OutOfOrder {
        field: String,
        reads: String,
        message: String,
    },
    /// the result isn't a number (dividing by zero, say), so the field is left empty
    NotANumber { field: String, message: String },
    /// the script took too many steps and was stopped
    TooLong { field: String, message: String },
}

/// the whole form's logic.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FormLogic {
    /// every field, with or without logic, in form order
    pub fields: Vec<FieldLogic>,
    /// calculated fields in the order they are worked out
    pub calculation_order: Vec<String>,
    pub issues: Vec<CalculationIssue>,
    /// "This form has a custom script Letters doesn’t run: a, b", when it has any
    pub custom_script_note: Option<String>,
    /// fields with custom scripts Letters leaves alone
    pub fields_with_custom_scripts: Vec<String>,
}

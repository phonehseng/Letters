//! what a recalculation run shares between all the formulas it
//! evaluates: the workbook, the parsed formulas, which cells still wait
//! to be calculated, results so far, spills, and the clock and random
//! numbers volatile functions use.

use std::cell::{Cell, RefCell};
use std::collections::{BTreeMap, HashMap, HashSet};

use crate::address::Range;
use crate::formula::Expr;
use crate::model::{Value, Workbook};

/// a cell anywhere in the workbook: (sheet, row, column)
pub type CellKey = (usize, u32, u32);

/// a formula as the engine keeps it
#[derive(Clone, Debug)]
pub struct ParsedFormula {
    pub expr: Option<Expr>,
    /// files written without dynamic arrays mean "one value per cell":
    /// a range in a value's place is reduced to the cell in line with
    /// the formula (excel's implicit intersection)
    pub legacy: bool,
    /// an array formula's fixed range (ctrl shift enter style)
    pub array_range: Option<Range>,
    /// functions letters does not have; when set the cell keeps its
    /// stored value and is reported as not recalculated
    pub unsupported: Vec<String>,
    pub volatile: bool,
    /// the areas the formula reads: (sheet, range)
    pub precedents: Vec<(usize, Range)>,
}

impl ParsedFormula {
    pub fn can_calculate(&self) -> bool {
        self.expr.is_some() && self.unsupported.is_empty()
    }
}

/// the moving parts of one recalculation
#[derive(Default)]
pub struct RunState {
    /// formula cells still to be calculated
    pub pending: HashSet<CellKey>,
    /// the same cells ordered by (sheet, column, row), to find the ones
    /// inside a range quickly
    pub pending_by_column: std::collections::BTreeSet<(usize, u32, u32)>,
    /// cells being calculated right now; meeting one again is a cycle
    pub in_progress: HashSet<CellKey>,
    /// new values of formula cells
    pub results: HashMap<CellKey, Value>,
    /// values of cells filled by a spilling formula: (sheet, row, col)
    pub spill_values: BTreeMap<CellKey, Value>,
    /// where each spilling formula spilled this time
    pub new_spills: HashMap<CellKey, Range>,
    /// where formulas showing #SPILL! wanted to spill
    pub blocked_spills: HashMap<CellKey, Range>,
    pub circular: Vec<CellKey>,
}

impl RunState {
    pub fn mark_pending(&mut self, key: CellKey) {
        if self.pending.insert(key) {
            self.pending_by_column.insert((key.0, key.2, key.1));
        }
    }

    pub fn finish(&mut self, key: CellKey) {
        if self.pending.remove(&key) {
            self.pending_by_column.remove(&(key.0, key.2, key.1));
        }
    }

    /// pending formula cells inside a range
    pub fn pending_in(&self, sheet: usize, range: &Range) -> Vec<CellKey> {
        let mut found = Vec::new();
        if range.cols() > 64 {
            // a wide range: looking through every pending cell is cheaper
            // than asking for each of its columns
            for key in &self.pending {
                if key.0 == sheet && range.contains(crate::address::CellRef::new(key.1, key.2)) {
                    found.push(*key);
                }
            }
            return found;
        }
        for col in range.start.col..=range.end.col {
            let low = (sheet, col, range.start.row);
            let high = (sheet, col, range.end.row);
            found.extend(
                self.pending_by_column
                    .range(low..=high)
                    .map(|(s, c, r)| (*s, *r, *c)),
            );
        }
        found
    }
}

/// shared, read-only for the run except through the cells inside
pub struct Context<'a> {
    pub book: &'a Workbook,
    pub formulas: &'a HashMap<CellKey, ParsedFormula>,
    /// where formulas spilled at the end of the last run
    pub old_spills: &'a HashMap<CellKey, Range>,
    pub state: RefCell<RunState>,
    /// now as a serial number in local time
    pub now: f64,
    random: Cell<u64>,
    /// defined names parsed once per run
    pub names: RefCell<HashMap<String, Option<Expr>>>,
    /// each sheet's used range, worked out once per run (it takes a pass
    /// over every row, and whole-column references ask for it often)
    used_ranges: RefCell<HashMap<usize, Option<Range>>>,
}

impl<'a> Context<'a> {
    pub fn new(
        book: &'a Workbook,
        formulas: &'a HashMap<CellKey, ParsedFormula>,
        old_spills: &'a HashMap<CellKey, Range>,
        now: f64,
        seed: u64,
    ) -> Context<'a> {
        Context {
            book,
            formulas,
            old_spills,
            state: RefCell::new(RunState::default()),
            now,
            random: Cell::new(seed | 1),
            names: RefCell::new(HashMap::new()),
            used_ranges: RefCell::new(HashMap::new()),
        }
    }

    pub fn used_range(&self, sheet: usize) -> Option<Range> {
        if let Some(known) = self.used_ranges.borrow().get(&sheet) {
            return *known;
        }
        let used = self
            .book
            .sheets
            .get(sheet)
            .and_then(|s| s.cells.used_range());
        self.used_ranges.borrow_mut().insert(sheet, used);
        used
    }

    /// a uniform random number in [0, 1) (xorshift64*: fast and good
    /// enough for RAND; nothing here needs a cryptographic source)
    pub fn random(&self) -> f64 {
        let mut x = self.random.get();
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.random.set(x);
        let mixed = x.wrapping_mul(0x2545_F491_4F6C_DD1D);
        (mixed >> 11) as f64 / (1u64 << 53) as f64
    }
}

/// the current local time as a spreadsheet serial
pub fn local_now_serial(system: crate::model::DateSystem) -> f64 {
    use chrono::{Datelike, Timelike};
    let now = chrono::Local::now();
    let day = crate::dates::serial_from_date(
        i64::from(now.year()),
        i64::from(now.month()),
        i64::from(now.day()),
        system,
    );
    let seconds = f64::from(now.num_seconds_from_midnight())
        + f64::from(now.nanosecond() / 1_000_000) / 1000.0;
    day + seconds / 86_400.0
}

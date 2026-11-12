//! conditional formatting as the grid draws it: for the cells a view asks
//! for, which rules match and what they change (fill, font colour, bold,
//! italic, underline, strike, and data bars), the way excel decides it.
//!
//! - rules are asked in priority order (1 first). every matching rule
//!   applies, but where two set the same thing the earlier one wins, and
//!   a matching rule with "stop if true" ends the list for that cell.
//! - formulas are written for the top-left cell of a format's first range
//!   and move with the cell, as a formula filled across the range would.
//! - rules that compare a cell with its range (top 10, average,
//!   duplicates, colour scales, data bars) look at the range once per
//!   request, so a rule over a whole column costs one pass, not one per
//!   cell.
//! - icon sets are not drawn (see the README).

mod differential;
mod matching;
mod scales;
mod statistics;
#[cfg(test)]
mod tests;

use std::cell::OnceCell;
use std::collections::HashMap;

use serde::Serialize;

use crate::address::{CellRef, Range, MAX_COLS};
use crate::calc::context::{local_now_serial, Context};
use crate::calc::value::to_number;
use crate::model::{ConditionalFormat, ConditionalRule, Sheet, Value, Workbook};

pub use differential::{read_differential, DifferentialLook};
pub use scales::DataBarLook;

use matching::{is_constant, Comparison, Operand, Place, Test, TextTest};
use scales::{ColorScale, DataBar, ResolvedBar, ResolvedScale};
use statistics::{value_counts, Numbers};

/// what the rules change in one cell; none leaves the cell's own look
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConditionalLook {
    /// "#RRGGBB", over the cell's own fill
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fill: Option<String>,
    /// "#RRGGBB", over the cell's own text colour
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bold: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub italic: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub underline: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub strike: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bar: Option<DataBarLook>,
}

impl ConditionalLook {
    pub fn is_empty(&self) -> bool {
        *self == ConditionalLook::default()
    }

    /// takes what `format` sets and nothing set before has
    fn add(&mut self, format: &DifferentialLook) {
        fill_in(&mut self.fill, &format.fill);
        fill_in(&mut self.color, &format.color);
        fill_in(&mut self.bold, &format.bold);
        fill_in(&mut self.italic, &format.italic);
        fill_in(&mut self.underline, &format.underline);
        fill_in(&mut self.strike, &format.strike);
    }
}

/// an earlier rule (a higher priority) keeps what it set
fn fill_in<T: Clone>(slot: &mut Option<T>, offered: &Option<T>) {
    if slot.is_none() {
        slot.clone_from(offered);
    }
}

/// what a matching rule does to the cell
enum Effect {
    Format(DifferentialLook),
    Scale(ResolvedScale),
    Bar(ResolvedBar),
    /// a rule kind that is not drawn (icon sets)
    Nothing,
}

/// one rule ready to be asked about cells
struct Prepared<'r> {
    ranges: &'r [Range],
    origin: CellRef,
    stop_if_true: bool,
    test: Test,
    effect: Effect,
}

impl Prepared<'_> {
    fn covers(&self, at: CellRef) -> bool {
        self.ranges.iter().any(|range| range.contains(at))
    }
}

/// the conditional look of every cell in `range` that a rule changes,
/// blank cells included (a "less than 50" rule marks empty cells too, as
/// excel does), in row order
pub fn looks_in(book: &Workbook, sheet: usize, range: Range) -> Vec<(CellRef, ConditionalLook)> {
    let Some(target) = book.sheets.get(sheet) else {
        return Vec::new();
    };
    let mut wanted: Vec<(&ConditionalFormat, &ConditionalRule)> = target
        .conditional_formats
        .iter()
        .filter(|format| format.ranges.iter().any(|r| r.intersects(&range)))
        .flat_map(|format| format.rules.iter().map(move |rule| (format, rule)))
        .collect();
    if wanted.is_empty() {
        return Vec::new();
    }
    // a stable sort keeps the file's order between rules of equal priority
    wanted.sort_by_key(|(_, rule)| rule.priority);
    let formulas = HashMap::new();
    let spills = HashMap::new();
    let ctx = Context::new(
        book,
        &formulas,
        &spills,
        local_now_serial(book.date_system),
        1,
    );
    let mut setup = Setup {
        book,
        sheet,
        target,
        ctx: &ctx,
        used: OnceCell::new(),
        formats: HashMap::new(),
    };
    let rules: Vec<Prepared> = wanted
        .into_iter()
        .map(|(format, rule)| setup.prepare(format, rule))
        .collect();
    let Some(area) = covered_area(&rules, range) else {
        return Vec::new();
    };
    let mut looks = Vec::new();
    for row in area.start.row..=area.end.row {
        for col in area.start.col..=area.end.col {
            let at = CellRef::new(row, col);
            let look = look_of(&rules, &setup, at);
            if !look.is_empty() {
                looks.push((at, look));
            }
        }
    }
    looks
}

/// the smallest range holding every part of the view a rule covers, so
/// cells no rule reaches are never visited
fn covered_area(rules: &[Prepared], view: Range) -> Option<Range> {
    rules
        .iter()
        .flat_map(|rule| rule.ranges.iter())
        .filter_map(|range| range.intersection(&view))
        .reduce(|a, b| {
            Range::from_bounds(
                a.start.row.min(b.start.row),
                a.start.col.min(b.start.col),
                a.end.row.max(b.end.row),
                a.end.col.max(b.end.col),
            )
        })
}

fn look_of(rules: &[Prepared], setup: &Setup, at: CellRef) -> ConditionalLook {
    let value = setup.target.cells.value(at.row, at.col);
    let mut look = ConditionalLook::default();
    for rule in rules.iter().filter(|rule| rule.covers(at)) {
        let place = Place {
            ctx: setup.ctx,
            sheet: setup.sheet,
            at,
            origin: rule.origin,
        };
        if !rule.test.matches(value, &place) {
            continue;
        }
        match &rule.effect {
            Effect::Format(format) => look.add(format),
            Effect::Scale(scale) => {
                if let Value::Number(n) = value {
                    fill_in(&mut look.fill, &Some(scale.colour(*n).css()));
                }
            }
            Effect::Bar(bar) => {
                if let Value::Number(n) = value {
                    fill_in(&mut look.bar, &Some(bar.look(*n)));
                }
            }
            Effect::Nothing => {}
        }
        if rule.stop_if_true {
            break;
        }
    }
    look
}

/// what preparing rules shares: the sheet's used range (found once, and
/// only when a rule needs range statistics) and the formats read so far
struct Setup<'s, 'a> {
    book: &'a Workbook,
    sheet: usize,
    target: &'a Sheet,
    ctx: &'s Context<'a>,
    used: OnceCell<Option<Range>>,
    formats: HashMap<u32, DifferentialLook>,
}

impl<'s, 'a> Setup<'s, 'a> {
    /// the rows that hold cells, across every column: enough to cut a
    /// whole-column range short, and found from the first and last row
    /// alone (the sheet's used range would visit every row for columns)
    fn used(&self) -> Option<Range> {
        *self.used.get_or_init(|| {
            let mut rows = self.target.cells.row_numbers();
            let first = rows.next()?;
            let last = rows.next_back().unwrap_or(first);
            Some(Range::from_bounds(first, 0, last, MAX_COLS - 1))
        })
    }

    /// the numbers of a format's ranges, read once for the rule asking
    fn numbers(&self, format: &ConditionalFormat) -> Numbers {
        Numbers::in_ranges(self.target, &format.ranges, self.used())
    }

    fn format(&mut self, index: Option<u32>) -> DifferentialLook {
        let Some(index) = index else {
            return DifferentialLook::default();
        };
        let styles = &self.book.styles;
        self.formats
            .entry(index)
            .or_insert_with(|| {
                styles
                    .differential
                    .get(index as usize)
                    .map(|xml| read_differential(xml, &styles.theme_colors))
                    .unwrap_or_default()
            })
            .clone()
    }

    /// a formula's value at the range's top-left cell, as a number (for
    /// scale points written as formulas)
    fn number_at(&self, origin: CellRef, text: &str) -> Option<f64> {
        let expr = crate::formula::parse(text).ok()?;
        let place = Place {
            ctx: self.ctx,
            sheet: self.sheet,
            at: origin,
            origin,
        };
        to_number(&place.evaluate(&expr), self.book.date_system).ok()
    }

    fn prepare<'r>(
        &mut self,
        format: &'r ConditionalFormat,
        rule: &'r ConditionalRule,
    ) -> Prepared<'r> {
        let origin = format.ranges.first().map(|r| r.start).unwrap_or_default();
        let (test, effect) = match rule.kind.as_str() {
            "colorScale" => self.color_scale(format, rule, origin),
            "dataBar" => self.data_bar(format, rule, origin),
            // icon sets are left out (see the module notes)
            "iconSet" => (Test::Never, Effect::Nothing),
            _ => (
                self.test(format, rule, origin),
                Effect::Format(self.format(rule.format)),
            ),
        };
        Prepared {
            ranges: &format.ranges,
            origin,
            stop_if_true: rule.stop_if_true,
            test,
            effect,
        }
    }

    fn color_scale(
        &self,
        format: &ConditionalFormat,
        rule: &ConditionalRule,
        origin: CellRef,
    ) -> (Test, Effect) {
        let details = rule.details_xml.as_deref().unwrap_or("");
        let mut numbers = self.numbers(format);
        let formula = |text: &str| self.number_at(origin, text);
        match ColorScale::read(details, &self.book.styles.theme_colors)
            .and_then(|scale| ResolvedScale::new(&scale, &mut numbers, &formula))
        {
            Some(scale) => (Test::Number, Effect::Scale(scale)),
            None => (Test::Never, Effect::Nothing),
        }
    }

    fn data_bar(
        &self,
        format: &ConditionalFormat,
        rule: &ConditionalRule,
        origin: CellRef,
    ) -> (Test, Effect) {
        let details = rule.details_xml.as_deref().unwrap_or("");
        let mut numbers = self.numbers(format);
        let formula = |text: &str| self.number_at(origin, text);
        match DataBar::read(details, self.target, &self.book.styles.theme_colors)
            .and_then(|bar| ResolvedBar::new(bar, &mut numbers, &formula))
        {
            Some(bar) => (Test::Number, Effect::Bar(bar)),
            None => (Test::Never, Effect::Nothing),
        }
    }

    /// the test of a rule that applies a differential format
    fn test(&self, format: &ConditionalFormat, rule: &ConditionalRule, origin: CellRef) -> Test {
        let attribute = |name: &str| {
            rule.attributes
                .iter()
                .find(|(key, _)| key == name)
                .map(|(_, value)| value.as_str())
        };
        let on = |name: &str, default: bool| match attribute(name) {
            Some(value) => value == "1" || value == "true",
            None => default,
        };
        let text_test = |how: TextTest| match &rule.text {
            Some(text) => Test::Text(how, text.to_lowercase()),
            // without the text excel keeps the formula it made from it
            None => self.formula_test(rule),
        };
        match rule.kind.as_str() {
            "cellIs" => self.comparison(rule, origin),
            "containsText" => text_test(TextTest::Contains),
            "notContainsText" => text_test(TextTest::NotContains),
            "beginsWith" => text_test(TextTest::BeginsWith),
            "endsWith" => text_test(TextTest::EndsWith),
            "containsBlanks" => Test::Blanks(true),
            "notContainsBlanks" => Test::Blanks(false),
            "containsErrors" => Test::Errors(true),
            "notContainsErrors" => Test::Errors(false),
            "top10" => {
                let rank = attribute("rank").and_then(|r| r.parse().ok()).unwrap_or(10);
                let mut numbers = self.numbers(format);
                top_or_bottom(
                    &mut numbers,
                    rank,
                    on("percent", false),
                    on("bottom", false),
                )
            }
            "aboveAverage" => {
                let numbers = self.numbers(format);
                let deviations = attribute("stdDev")
                    .and_then(|d| d.parse().ok())
                    .unwrap_or(0.0);
                average_test(
                    &numbers,
                    on("aboveAverage", true),
                    on("equalAverage", false),
                    deviations,
                )
            }
            "duplicateValues" | "uniqueValues" => Test::Counted {
                counts: value_counts(self.target, &format.ranges, self.used()),
                unique: rule.kind == "uniqueValues",
            },
            // "expression", and "timePeriod" whose formula excel writes
            // out (TODAY()-1 and so on)
            _ => self.formula_test(rule),
        }
    }

    fn formula_test(&self, rule: &ConditionalRule) -> Test {
        rule.formulas
            .first()
            .and_then(|text| crate::formula::parse(text).ok())
            .map(Test::Formula)
            .unwrap_or(Test::Never)
    }

    fn comparison(&self, rule: &ConditionalRule, origin: CellRef) -> Test {
        let Some(comparison) = rule.operator.as_deref().and_then(Comparison::parse) else {
            return Test::Never;
        };
        let place = Place {
            ctx: self.ctx,
            sheet: self.sheet,
            at: origin,
            origin,
        };
        let operands: Option<Vec<Operand>> = rule
            .formulas
            .iter()
            .map(|text| {
                let expr = crate::formula::parse(text).ok()?;
                Some(if is_constant(&expr) {
                    // worked out once, not for every cell
                    Operand::Fixed(place.evaluate(&expr))
                } else {
                    Operand::Moving(expr)
                })
            })
            .collect();
        match operands {
            Some(operands) if !operands.is_empty() => Test::Compare(comparison, operands),
            _ => Test::Never,
        }
    }
}

/// top or bottom n (or n percent): the n-th number from that end is the
/// limit, and ties with it match too, as in excel
fn top_or_bottom(numbers: &mut Numbers, rank: u32, percent: bool, bottom: bool) -> Test {
    let total = numbers.len();
    if total == 0 {
        return Test::Never;
    }
    let count = if percent {
        // a percentage of the numbers, rounded down but at least one
        ((total as f64 * f64::from(rank) / 100.0).floor() as usize).max(1)
    } else {
        rank as usize
    };
    let count = count.clamp(1, total);
    let limit = if bottom {
        numbers.nth_smallest(count - 1)
    } else {
        numbers.nth_smallest(total - count)
    };
    match (limit, bottom) {
        (Some(limit), true) => Test::AtMost(limit),
        (Some(limit), false) => Test::AtLeast(limit),
        (None, _) => Test::Never,
    }
}

/// above or below the average, or some standard deviations past it
fn average_test(numbers: &Numbers, above: bool, equal: bool, deviations: f64) -> Test {
    let Some((mean, deviation)) = numbers.mean_and_deviation() else {
        return Test::Never;
    };
    let shift = deviations * deviation;
    if above {
        Test::Above {
            limit: mean + shift,
            equal,
        }
    } else {
        Test::Below {
            limit: mean - shift,
            equal,
        }
    }
}

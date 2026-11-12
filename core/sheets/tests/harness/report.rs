//! comparing letters' results with excel's and libreoffice's, cell by
//! cell, and writing the agreement tables and every disagreement.

use std::collections::BTreeMap;

use letters_sheets::model::Value;

/// one case's three answers
pub struct Outcome {
    pub family: String,
    pub formula: String,
    pub ours: Value,
    pub excel: Option<Value>,
    pub libreoffice: Option<Value>,
}

/// two results count as the same when they are the same kind and, for
/// numbers, equal to 1 part in 10^12 (beyond what excel shows)
pub fn same(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Number(x), Value::Number(y)) => {
            let scale = x.abs().max(y.abs());
            (x - y).abs() <= scale * 1e-12 || (x - y).abs() < 1e-300
        }
        // a formula that shows nothing is 0 to excel's com interface
        (Value::Blank, Value::Number(n)) | (Value::Number(n), Value::Blank) => *n == 0.0,
        (Value::Blank, Value::Text(t)) | (Value::Text(t), Value::Blank) => t.is_empty(),
        _ => a == b,
    }
}

pub fn show(value: &Option<Value>) -> String {
    match value {
        None => "(not run)".into(),
        Some(Value::Number(n)) => format!("{n}"),
        Some(Value::Text(t)) => format!("\"{t}\""),
        Some(Value::Bool(b)) => if *b { "TRUE" } else { "FALSE" }.into(),
        Some(Value::Error(e)) => e.as_str().into(),
        Some(Value::Blank) => "(blank)".into(),
    }
}

#[derive(Default)]
struct Tally {
    cases: usize,
    with_excel: usize,
    with_libreoffice: usize,
    apps_differ: usize,
}

pub fn markdown(outcomes: &[Outcome]) -> String {
    let mut tallies: BTreeMap<String, Tally> = BTreeMap::new();
    for outcome in outcomes {
        let tally = tallies.entry(outcome.family.clone()).or_default();
        tally.cases += 1;
        tally.with_excel += usize::from(
            outcome
                .excel
                .as_ref()
                .is_some_and(|e| same(&outcome.ours, e)),
        );
        tally.with_libreoffice += usize::from(
            outcome
                .libreoffice
                .as_ref()
                .is_some_and(|l| same(&outcome.ours, l)),
        );
        if let (Some(e), Some(l)) = (&outcome.excel, &outcome.libreoffice) {
            tally.apps_differ += usize::from(!same(e, l));
        }
    }
    let mut out = String::from("| Family | Cases | Same as Excel | Same as LibreOffice | Excel and LibreOffice differ |\n|---|---|---|---|---|\n");
    let mut total = Tally::default();
    for (family, tally) in &tallies {
        out.push_str(&row(family, tally));
        total.cases += tally.cases;
        total.with_excel += tally.with_excel;
        total.with_libreoffice += tally.with_libreoffice;
        total.apps_differ += tally.apps_differ;
    }
    out.push_str(&row("**all**", &total));
    out.push_str("\n### Disagreements\n\n| Family | Formula | Letters | Excel | LibreOffice |\n|---|---|---|---|---|\n");
    for outcome in outcomes {
        let excel_ok = outcome
            .excel
            .as_ref()
            .is_none_or(|e| same(&outcome.ours, e));
        let lo_ok = outcome
            .libreoffice
            .as_ref()
            .is_none_or(|l| same(&outcome.ours, l));
        if excel_ok && lo_ok {
            continue;
        }
        out.push_str(&format!(
            "| {} | `{}` | {} | {} | {} |\n",
            outcome.family,
            outcome.formula.replace('|', "\\|"),
            show(&Some(outcome.ours.clone())).replace('|', "\\|"),
            show(&outcome.excel).replace('|', "\\|"),
            show(&outcome.libreoffice).replace('|', "\\|"),
        ));
    }
    out
}

fn percent(part: usize, whole: usize) -> String {
    if whole == 0 {
        return "-".into();
    }
    format!("{part} ({:.1}%)", 100.0 * part as f64 / whole as f64)
}

fn row(family: &str, tally: &Tally) -> String {
    format!(
        "| {family} | {} | {} | {} | {} |\n",
        tally.cases,
        percent(tally.with_excel, tally.cases),
        percent(tally.with_libreoffice, tally.cases),
        tally.apps_differ
    )
}

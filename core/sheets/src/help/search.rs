//! formula autocomplete: functions found by the start of their name, by a
//! loose spelling, and by what someone wants to do ("average", "count if",
//! "look up", "combine text", "today's date").

use serde::Serialize;

use super::catalogue::{catalogue, FunctionInfo};

/// words people use for what they want, and the functions that do it
const INTENTS: &[(&str, &[&str])] = &[
    ("add total sum plus", &["SUM", "SUMIF", "SUMIFS"]),
    (
        "average mean",
        &["AVERAGE", "AVERAGEIF", "AVERAGEIFS", "MEDIAN"],
    ),
    (
        "count how many number of",
        &["COUNT", "COUNTA", "COUNTIF", "COUNTIFS", "COUNTBLANK"],
    ),
    (
        "count if condition criteria matching",
        &["COUNTIF", "COUNTIFS", "SUMIF", "SUMIFS"],
    ),
    (
        "look up lookup find match search table",
        &["XLOOKUP", "VLOOKUP", "INDEX", "MATCH", "HLOOKUP"],
    ),
    (
        "combine join concatenate merge text glue",
        &["TEXTJOIN", "CONCAT", "CONCATENATE"],
    ),
    ("today date now current time", &["TODAY", "NOW", "DATE"]),
    (
        "round decimals",
        &["ROUND", "ROUNDUP", "ROUNDDOWN", "MROUND"],
    ),
    (
        "biggest largest highest maximum top",
        &["MAX", "LARGE", "MAXIFS"],
    ),
    (
        "smallest lowest minimum bottom",
        &["MIN", "SMALL", "MINIFS"],
    ),
    (
        "if condition choose decide",
        &["IF", "IFS", "SWITCH", "IFERROR"],
    ),
    ("error hide blank instead", &["IFERROR", "IFNA"]),
    ("percent percentage share", &["PERCENTILE.INC", "TEXT"]),
    (
        "loan mortgage payment interest",
        &["PMT", "IPMT", "PPMT", "RATE", "NPER"],
    ),
    (
        "savings future investment worth",
        &["FV", "PV", "NPV", "IRR"],
    ),
    (
        "age years between dates difference",
        &["DATEDIF", "YEARFRAC", "DAYS"],
    ),
    (
        "working days business weekday",
        &["NETWORKDAYS", "WORKDAY", "WEEKDAY"],
    ),
    (
        "first letters characters left part",
        &["LEFT", "MID", "RIGHT"],
    ),
    ("spaces clean tidy", &["TRIM", "CLEAN"]),
    (
        "capital uppercase lowercase case",
        &["UPPER", "LOWER", "PROPER"],
    ),
    ("replace swap change text", &["SUBSTITUTE", "REPLACE"]),
    ("unique distinct duplicates", &["UNIQUE", "COUNTIF"]),
    ("sort order", &["SORT", "LARGE", "SMALL", "RANK.EQ"]),
    ("filter keep rows", &["FILTER"]),
    ("rank position place", &["RANK.EQ", "MATCH", "XMATCH"]),
    ("random dice", &["RAND", "RANDBETWEEN"]),
    (
        "spread deviation variance",
        &["STDEV.S", "STDEV.P", "VAR.S"],
    ),
    ("number list series sequence", &["SEQUENCE", "ROW"]),
];

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Completion {
    pub name: String,
    pub description: String,
    /// why it was offered: "name", "spelling" or "purpose"
    pub matched_by: &'static str,
}

/// edit distance between two short words (for loose spellings)
fn distance(a: &str, b: &str) -> usize {
    let b: Vec<char> = b.chars().collect();
    let mut row: Vec<usize> = (0..=b.len()).collect();
    for (i, ca) in a.chars().enumerate() {
        let mut previous = row[0];
        row[0] = i + 1;
        for j in 0..b.len() {
            let current = row[j + 1];
            row[j + 1] = if ca == b[j] {
                previous
            } else {
                1 + previous.min(row[j]).min(row[j + 1])
            };
            previous = current;
        }
    }
    row[b.len()]
}

fn completion(info: &FunctionInfo, matched_by: &'static str) -> Completion {
    Completion {
        name: info.name.clone(),
        description: info.description.clone(),
        matched_by,
    }
}

/// what to offer for `typed`: names starting with it first, then close
/// spellings, then functions for what the words describe
pub fn complete(typed: &str, limit: usize) -> Vec<Completion> {
    let query = typed.trim().trim_start_matches('=').to_ascii_uppercase();
    if query.is_empty() {
        return Vec::new();
    }
    let all = catalogue();
    let mut found: Vec<Completion> = Vec::new();
    let add = |info: &FunctionInfo, why: &'static str, found: &mut Vec<Completion>| {
        if !found.iter().any(|c| c.name == info.name) {
            found.push(completion(info, why));
        }
    };
    for info in all.iter().filter(|f| f.name.starts_with(&query)) {
        add(info, "name", &mut found);
    }
    let lower = typed.trim().to_lowercase();
    let words: Vec<&str> = lower.split_whitespace().collect();
    for (keys, names) in INTENTS {
        let hits = words
            .iter()
            .filter(|w| {
                w.len() > 1
                    && keys.split(' ').any(|k| {
                        k == **w || (w.len() >= 4 && (k.starts_with(*w) || w.starts_with(k)))
                    })
            })
            .count();
        if hits > 0 && (hits == words.len() || words.len() <= 2) {
            for name in *names {
                if let Some(info) = all.iter().find(|f| f.name == *name) {
                    add(info, "purpose", &mut found);
                }
            }
        }
    }
    if query.len() >= 3 && !query.contains(' ') {
        let mut close: Vec<(&FunctionInfo, usize)> = all
            .iter()
            .map(|f| (f, distance(&query, &f.name)))
            .filter(|(f, d)| *d <= 2 && *d < f.name.len() / 2 + 1)
            .collect();
        close.sort_by_key(|(_, d)| *d);
        for (info, _) in close {
            add(info, "spelling", &mut found);
        }
    }
    found.truncate(limit);
    found
}

/// the closest function name to an unknown one ("VLOKUP" -> "VLOOKUP")
pub fn closest_function(name: &str) -> Option<String> {
    let upper = name.to_ascii_uppercase();
    catalogue()
        .into_iter()
        .map(|f| {
            let d = distance(&upper, &f.name);
            (f.name, d)
        })
        .filter(|(_, d)| *d <= 2)
        .min_by_key(|(_, d)| *d)
        .map(|(name, _)| name)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(typed: &str) -> Vec<String> {
        complete(typed, 8).into_iter().map(|c| c.name).collect()
    }

    #[test]
    fn finds_by_name_spelling_and_purpose() {
        assert_eq!(
            names("SUMI")[..2],
            ["SUMIF".to_string(), "SUMIFS".to_string()]
        );
        assert!(names("average").contains(&"AVERAGE".to_string()));
        assert!(names("count if").contains(&"COUNTIF".to_string()));
        assert!(names("look up").contains(&"XLOOKUP".to_string()));
        assert!(names("combine text").contains(&"TEXTJOIN".to_string()));
        assert!(names("today's date").contains(&"TODAY".to_string()));
        assert!(names("vlokup").contains(&"VLOOKUP".to_string()));
        assert_eq!(closest_function("SUMM").as_deref(), Some("SUM"));
        assert_eq!(closest_function("qwertyuiop"), None);
    }
}

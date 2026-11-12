//! summaries and key points, including documents too long to read at once.
//!
//! **map-reduce.** when the pages fit in one request, they are sent in
//! one go. when they don't:
//! 1. *map*: the pages are split into parts that each fit, and the model
//!    writes short notes on each part on its own;
//! 2. *reduce*: the notes are combined into the final summary. if even
//!    the notes are too long, they are first combined into shorter notes,
//!    as many rounds as it takes.
//!
//! only one part's text and the notes are held at a time, so memory stays
//! bounded by the document text the caller already has.

use serde::Serialize;

use crate::cancel::Stage;
use crate::chunking::Piece;
use crate::document::PageRange;
use crate::prompts::{self, build_request};
use crate::tasks::budget::{self, page_passages, Passage};
use crate::tasks::runner::Runner;
use crate::AiError;

/// how long a summary should be.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SummaryLength {
    Short,
    Medium,
    Detailed,
}

impl SummaryLength {
    fn prompt(self) -> &'static str {
        match self {
            SummaryLength::Short => prompts::SUMMARY_SHORT,
            SummaryLength::Medium => prompts::SUMMARY_MEDIUM,
            SummaryLength::Detailed => prompts::SUMMARY_DETAILED,
        }
    }

    fn answer_tokens(self) -> usize {
        match self {
            SummaryLength::Short => 200,
            SummaryLength::Medium => 450,
            SummaryLength::Detailed => 900,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Summary {
    pub text: String,
    pub pages: PageRange,
    /// how many parts were read separately (1 when it all fit at once).
    pub parts: usize,
}

/// one key point and the pages it names.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct KeyPoint {
    pub text: String,
    /// pages the point cites, checked to be inside the range read.
    pub pages: Vec<u32>,
}

/// tokens allowed for the notes on one part.
const PART_ANSWER_TOKENS: usize = 300;

/// the four instructions one map-reduce uses.
struct Instructions {
    /// when everything fits in one request.
    whole: String,
    /// notes on one part.
    part: &'static str,
    /// the final step from notes.
    from_notes: String,
    /// shrinking notes that are still too long.
    shrink_notes: &'static str,
    answer_tokens: usize,
}

pub fn summarize(
    runner: &Runner,
    pieces: &[Piece],
    range: PageRange,
    length: SummaryLength,
) -> Result<Summary, AiError> {
    let instructions = Instructions {
        whole: length.prompt().to_string(),
        part: prompts::SUMMARY_PART,
        from_notes: format!("{}\n{}", prompts::SUMMARY_COMBINE, length.prompt()),
        shrink_notes: prompts::SUMMARY_COMBINE_NOTES,
        answer_tokens: length.answer_tokens(),
    };
    let (text, parts) = map_reduce(runner, pieces, &instructions)?;
    Ok(Summary {
        text: text.trim().to_string(),
        pages: range,
        parts,
    })
}

pub fn key_points(
    runner: &Runner,
    pieces: &[Piece],
    range: PageRange,
    count: usize,
) -> Result<Vec<KeyPoint>, AiError> {
    let count = count.clamp(1, 20);
    let instructions = Instructions {
        whole: prompts::with_count(prompts::KEY_POINTS, count),
        part: prompts::KEY_POINTS_PART,
        from_notes: prompts::with_count(prompts::KEY_POINTS_COMBINE, count),
        shrink_notes: prompts::KEY_POINTS_COMBINE_NOTES,
        answer_tokens: 60 * count + 60,
    };
    let (text, _) = map_reduce(runner, pieces, &instructions)?;
    Ok(parse_key_points(&text, range, count))
}

/// runs the whole thing and returns the final text and the number of parts.
fn map_reduce(
    runner: &Runner,
    pieces: &[Piece],
    instructions: &Instructions,
) -> Result<(String, usize), AiError> {
    let whole_budget = runner.input_chars(
        instructions.answer_tokens,
        prompts::overhead_chars(&instructions.whole, None),
    )?;
    let part_budget = runner.input_chars(
        PART_ANSWER_TOKENS,
        prompts::overhead_chars(instructions.part, None),
    )?;
    let passages = page_passages(pieces, part_budget / 2);
    if budget::total_len(&passages) <= whole_budget {
        runner.step(0, 1, Stage::Writing);
        let request = build_request(
            &instructions.whole,
            &passages,
            None,
            instructions.answer_tokens,
        );
        let answer = runner.complete_shown(&request)?;
        runner.step(1, 1, Stage::Writing);
        return Ok((answer.text, 1));
    }
    let groups = budget::pack(&passages, part_budget)?;
    let mut notes = Vec::with_capacity(groups.len());
    for (index, group) in groups.iter().enumerate() {
        runner.step(index, groups.len(), Stage::Reading);
        let request = build_request(
            instructions.part,
            &passages[group.clone()],
            None,
            PART_ANSWER_TOKENS,
        );
        notes.push(runner.complete_hidden(&request)?.text.trim().to_string());
    }
    runner.step(groups.len(), groups.len(), Stage::Reading);
    let text = reduce(runner, notes, instructions)?;
    Ok((text, groups.len()))
}

/// combines notes until they fit, then writes the final answer from them.
fn reduce(
    runner: &Runner,
    mut notes: Vec<String>,
    instructions: &Instructions,
) -> Result<String, AiError> {
    loop {
        runner.check_cancelled()?;
        let labelled = label_notes(&notes);
        let final_budget = runner.input_chars(
            instructions.answer_tokens,
            prompts::overhead_chars(&instructions.from_notes, None),
        )?;
        if budget::total_len(&labelled) <= final_budget {
            runner.step(0, 1, Stage::Writing);
            let request = build_request(
                &instructions.from_notes,
                &labelled,
                None,
                instructions.answer_tokens,
            );
            let answer = runner.complete_shown(&request)?;
            runner.step(1, 1, Stage::Writing);
            return Ok(answer.text);
        }
        let shrink_budget = runner.input_chars(
            PART_ANSWER_TOKENS,
            prompts::overhead_chars(instructions.shrink_notes, None),
        )?;
        let groups = budget::pack(&labelled, shrink_budget)?;
        // each round must leave fewer notes, or it would never end
        if groups.len() >= notes.len() {
            return Err(AiError::OutOfContext);
        }
        let mut shorter = Vec::with_capacity(groups.len());
        for (index, group) in groups.iter().enumerate() {
            runner.step(index, groups.len(), Stage::Combining);
            let request = build_request(
                instructions.shrink_notes,
                &labelled[group.clone()],
                None,
                PART_ANSWER_TOKENS,
            );
            shorter.push(runner.complete_hidden(&request)?.text.trim().to_string());
        }
        runner.step(groups.len(), groups.len(), Stage::Combining);
        notes = shorter;
    }
}

fn label_notes(notes: &[String]) -> Vec<Passage> {
    notes
        .iter()
        .enumerate()
        .map(|(index, note)| (format!("[notes on part {}]", index + 1), note.clone()))
        .collect()
}

/// reads "- point (p. 3)" lines; anything else the model wrote is ignored.
pub fn parse_key_points(text: &str, range: PageRange, count: usize) -> Vec<KeyPoint> {
    text.lines()
        .filter_map(|line| strip_bullet(line.trim()))
        .filter(|point| !point.is_empty())
        .take(count)
        .map(|point| KeyPoint {
            text: point.to_string(),
            pages: cited_pages(point)
                .into_iter()
                .filter(|page| range.contains(*page))
                .collect(),
        })
        .collect()
}

fn strip_bullet(line: &str) -> Option<&str> {
    for bullet in ["- ", "* ", "• "] {
        if let Some(rest) = line.strip_prefix(bullet) {
            return Some(rest.trim());
        }
    }
    // "1. point" or "1) point"
    let digits = line.chars().take_while(char::is_ascii_digit).count();
    if digits > 0 {
        let rest = &line[digits..];
        if let Some(rest) = rest.strip_prefix(". ").or_else(|| rest.strip_prefix(") ")) {
            return Some(rest.trim());
        }
    }
    None
}

/// the page numbers in "(p. 3)", "(pp. 3–4)" or "(p. 3, 5)".
pub fn cited_pages(text: &str) -> Vec<u32> {
    let mut pages = Vec::new();
    let mut rest = text;
    while let Some(open) = rest.find("(p") {
        let after = &rest[open + 2..];
        let Some(close) = after.find(')') else { break };
        let inside = &after[..close];
        // only "(p. ..." and "(pp. ...", not "(possibly ...)"
        if inside.starts_with(". ") || inside.starts_with("p. ") || inside.starts_with('.') {
            pages.extend(numbers_in(inside));
        }
        rest = &after[close..];
    }
    pages.sort_unstable();
    pages.dedup();
    pages
}

/// numbers and ranges such as "3–5" in a short piece of text.
fn numbers_in(text: &str) -> Vec<u32> {
    let mut found = Vec::new();
    let normalised = text.replace(['–', '—'], "-");
    for part in normalised.split([',', ' ', ';']) {
        let part = part.trim_matches(|c: char| !c.is_ascii_digit() && c != '-');
        match part.split_once('-') {
            Some((a, b)) => {
                if let (Ok(a), Ok(b)) = (a.parse::<u32>(), b.parse::<u32>()) {
                    // a range like 3-5, kept short so a typo can't list a thousand pages
                    if a <= b && b - a < 50 {
                        found.extend(a..=b);
                    }
                }
            }
            None => found.extend(part.parse::<u32>().ok()),
        }
    }
    found
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn key_points_are_read_from_bullets() {
        let text = "Here are the points:\n- Cells make energy (p. 2).\n* Plants use light (pp. 3–4)\n1. Water matters (p. 9)\n- Extra point (p. 1)";
        let points = parse_key_points(text, PageRange::new(1, 5), 3);
        assert_eq!(points.len(), 3);
        assert_eq!(points[0].pages, vec![2]);
        assert_eq!(points[1].pages, vec![3, 4]);
        // page 9 isn't in the range read, so it isn't offered as a jump
        assert_eq!(points[2].pages, Vec::<u32>::new());
    }

    #[test]
    fn page_references_are_found() {
        assert_eq!(
            cited_pages("one (p. 3) two (p. 5, 7) (possibly not)"),
            vec![3, 5, 7]
        );
        assert_eq!(cited_pages("no pages here"), Vec::<u32>::new());
        assert_eq!(cited_pages("(pp. 10-12)"), vec![10, 11, 12]);
    }
}

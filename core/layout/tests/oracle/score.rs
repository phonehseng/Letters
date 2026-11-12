//! scores one layout against another (ours against an oracle, or one
//! oracle against the other) with the same measures throughout.
//!
//! lines are matched by identical text (spaces left out, see
//! `lines::normalise`), preferring the same page and the nearest baseline,
//! each line used once. a matched line broke at the same words in both.

use letters_layout::LayoutResult;

use super::lines::TextLine;
use super::pdf_glyphs::{Glyph, PdfPage};

/// one layout as the scores see it: its lines, and per page the x of its
/// vertical rules and the boxes of its pictures.
#[derive(Clone, Debug, Default)]
pub struct Laid {
    pub pages: usize,
    pub lines: Vec<TextLine>,
    pub rules: Vec<Vec<f64>>,
    pub pictures: Vec<Vec<[f64; 4]>>,
}

impl Laid {
    pub fn of_pdf(pages: &[PdfPage]) -> Laid {
        Laid {
            pages: pages.len(),
            lines: super::lines::lines_of_document(pages),
            rules: pages.iter().map(|p| p.vertical_rules.clone()).collect(),
            pictures: pages.iter().map(|p| p.pictures.clone()).collect(),
        }
    }
}

/// the lines of our own layout: its glyphs in drawing order, grouped by
/// the very rule that groups a pdf's glyphs (`lines::lines_of_page`), so
/// table cells on one baseline merge exactly as they do in an oracle's pdf.
pub fn lines_of_result(result: &LayoutResult) -> Laid {
    let pages = glyph_pages(result);
    let mut laid = Laid::of_pdf(&pages);
    laid.pages = result.pages.len();
    laid
}

/// our result as the pdf reader would see it: each glyph's first
/// character at its pen position, line by line in drawing order, with
/// the glyphs of a line in x order (leaders are runs of their own).
pub fn glyph_pages(result: &LayoutResult) -> Vec<PdfPage> {
    result
        .pages
        .iter()
        .map(|page| {
            let mut glyphs = Vec::new();
            for line in &page.lines {
                let mut placed: Vec<Glyph> = Vec::new();
                for run in &line.runs {
                    let chars: Vec<char> = run.text.chars().collect();
                    for (index, start) in run.clusters.iter().enumerate() {
                        let ch = chars.get(*start as usize).copied().unwrap_or(' ');
                        if ch.is_whitespace() || ch.is_control() {
                            continue;
                        }
                        placed.push(Glyph {
                            ch,
                            x: run.x[index],
                            y: run.y,
                            size: run.size,
                            font: String::new(),
                        });
                    }
                }
                placed.sort_by(|a, b| a.x.total_cmp(&b.x));
                glyphs.extend(placed);
            }
            PdfPage {
                width: page.width,
                height: page.height,
                glyphs,
                vertical_rules: page
                    .rects
                    .iter()
                    .filter(|r| r.width <= 2.6 && r.height >= 3.0)
                    .map(|r| r.x + r.width / 2.0)
                    .collect(),
                pictures: page
                    .images
                    .iter()
                    .map(|i| [i.x, i.y, i.width, i.height])
                    .collect(),
            }
        })
        .collect()
}

#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct Score {
    pub reference_pages: usize,
    pub candidate_pages: usize,
    /// documents scored, and how many have the same page count
    #[serde(default)]
    pub documents: usize,
    #[serde(default)]
    pub pages_match: usize,
    pub reference_lines: usize,
    /// lines of the reference with an identical line in the candidate
    pub same_lines: usize,
    /// pages whose first and last body text agree
    pub pages_compared: usize,
    pub page_breaks_agree: usize,
    /// over matched lines on the same page
    pub baseline: Stats,
    pub line_x: Stats,
    /// glyph x relative to its line's start
    pub glyph_x: Stats,
    /// each vertical rule of the reference (table column edges, borders)
    /// to the nearest one of the candidate on the same page
    #[serde(default)]
    pub rules: Stats,
    /// each picture of the reference to the nearest one of the candidate
    /// on the same page: the larger of the x and y differences
    #[serde(default)]
    pub pictures: Stats,
}

#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct Stats {
    pub count: usize,
    pub mean: f64,
    pub max: f64,
    pub p95: f64,
    /// every value, for combining percentiles exactly (not in the report)
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub values: Vec<f64>,
}

impl Stats {
    pub fn of(mut values: Vec<f64>) -> Stats {
        if values.is_empty() {
            return Stats::default();
        }
        values.sort_by(f64::total_cmp);
        let count = values.len();
        Stats {
            count,
            mean: values.iter().sum::<f64>() / count as f64,
            max: values[count - 1],
            p95: values[((count as f64 * 0.95).ceil() as usize).min(count) - 1],
            values,
        }
    }
}

impl Score {
    pub fn same_line_share(&self) -> f64 {
        if self.reference_lines == 0 {
            return 1.0;
        }
        self.same_lines as f64 / self.reference_lines as f64
    }

    pub fn page_break_share(&self) -> f64 {
        if self.pages_compared == 0 {
            return 1.0;
        }
        self.page_breaks_agree as f64 / self.pages_compared as f64
    }
}

/// the body text zone of each page, to tell headers and footers apart:
/// lines above `top` or below `bottom` are not body text.
#[derive(Clone, Copy, Debug)]
pub struct BodyZone {
    pub top: f64,
    pub bottom: f64,
}

/// a rule or picture with no partner nearer than this counts as this far
const UNMATCHED: f64 = 30.0;

/// scores `candidate` against `reference`.
pub fn score(reference: &Laid, candidate: &Laid, zone: BodyZone) -> Score {
    let (reference_lines, candidate_lines) = (&reference.lines, &candidate.lines);
    let mut used = vec![false; candidate_lines.len()];
    let mut same = 0;
    let mut baseline = Vec::new();
    let mut line_x = Vec::new();
    let mut glyph_x = Vec::new();
    for line in reference_lines {
        let Some(found) = best_match(line, candidate_lines, &used) else {
            continue;
        };
        used[found] = true;
        same += 1;
        let other = &candidate_lines[found];
        if other.page != line.page {
            continue;
        }
        baseline.push((other.baseline - line.baseline).abs());
        line_x.push((other.x - line.x).abs());
        if other.xs.len() == line.xs.len() && !line.xs.is_empty() {
            for (a, b) in line.xs.iter().zip(&other.xs) {
                glyph_x.push(((a - line.xs[0]) - (b - other.xs[0])).abs());
            }
        }
    }
    let (pages_compared, page_breaks_agree) = page_breaks(
        reference_lines,
        candidate_lines,
        reference.pages.min(candidate.pages),
        zone,
    );
    Score {
        reference_pages: reference.pages,
        candidate_pages: candidate.pages,
        documents: 1,
        pages_match: usize::from(reference.pages == candidate.pages),
        reference_lines: reference_lines.len(),
        same_lines: same,
        pages_compared,
        page_breaks_agree,
        baseline: Stats::of(baseline),
        line_x: Stats::of(line_x),
        glyph_x: Stats::of(glyph_x),
        rules: Stats::of(rule_differences(&reference.rules, &candidate.rules)),
        pictures: Stats::of(picture_differences(
            &reference.pictures,
            &candidate.pictures,
        )),
    }
}

/// the reference's distinct rule positions per page, each to the nearest
/// candidate rule on the same page.
pub fn rule_differences(reference: &[Vec<f64>], candidate: &[Vec<f64>]) -> Vec<f64> {
    let mut out = Vec::new();
    for (page, rules) in reference.iter().enumerate() {
        let theirs = distinct(rules);
        let ours = distinct(candidate.get(page).map_or(&[][..], |v| v.as_slice()));
        for x in theirs {
            let nearest = ours.iter().map(|o| (o - x).abs()).fold(UNMATCHED, f64::min);
            out.push(nearest);
        }
    }
    out
}

/// positions closer than a third of a point are one rule (a cell's right
/// edge and the next cell's left edge drawn separately).
fn distinct(values: &[f64]) -> Vec<f64> {
    let mut sorted = values.to_vec();
    sorted.sort_by(f64::total_cmp);
    let mut out: Vec<f64> = Vec::new();
    for value in sorted {
        if out.last().is_none_or(|last| value - last > 0.33) {
            out.push(value);
        }
    }
    out
}

/// each reference picture to the closest candidate picture on its page.
pub fn picture_differences(reference: &[Vec<[f64; 4]>], candidate: &[Vec<[f64; 4]>]) -> Vec<f64> {
    let mut out = Vec::new();
    for (page, pictures) in reference.iter().enumerate() {
        let ours = candidate.get(page).map_or(&[][..], |v| v.as_slice());
        for picture in pictures {
            let nearest = ours
                .iter()
                .map(|o| (o[0] - picture[0]).abs().max((o[1] - picture[1]).abs()))
                .fold(UNMATCHED, f64::min);
            out.push(nearest);
        }
    }
    out
}

fn best_match(line: &TextLine, candidates: &[TextLine], used: &[bool]) -> Option<usize> {
    candidates
        .iter()
        .enumerate()
        .filter(|(index, other)| !used[*index] && other.text == line.text)
        .min_by(|(_, a), (_, b)| {
            let cost = |other: &TextLine| {
                (other.page as f64 - line.page as f64).abs() * 10_000.0
                    + (other.baseline - line.baseline).abs()
            };
            cost(a).total_cmp(&cost(b))
        })
        .map(|(index, _)| index)
}

/// the first and last few characters of each page's body text.
fn page_ends(lines: &[TextLine], page: usize, zone: BodyZone) -> Option<(String, String)> {
    let body: Vec<&TextLine> = lines
        .iter()
        .filter(|line| line.page == page && line.baseline > zone.top && line.baseline < zone.bottom)
        .collect();
    let first = body
        .iter()
        .min_by(|a, b| a.baseline.total_cmp(&b.baseline).then(a.x.total_cmp(&b.x)))?;
    let last = body
        .iter()
        .max_by(|a, b| a.baseline.total_cmp(&b.baseline).then(a.x.total_cmp(&b.x)))?;
    let head: String = first.text.chars().take(8).collect();
    let tail: String = {
        let chars: Vec<char> = last.text.chars().collect();
        chars[chars.len().saturating_sub(8)..].iter().collect()
    };
    Some((head, tail))
}

fn page_breaks(
    reference: &[TextLine],
    candidate: &[TextLine],
    pages: usize,
    zone: BodyZone,
) -> (usize, usize) {
    let mut compared = 0;
    let mut agree = 0;
    for page in 0..pages {
        let (Some(a), Some(b)) = (
            page_ends(reference, page, zone),
            page_ends(candidate, page, zone),
        ) else {
            continue;
        };
        compared += 1;
        if a == b {
            agree += 1;
        }
    }
    (compared, agree)
}

/// several documents' scores added up (percentiles over all values).
pub fn combine(scores: &[&Score]) -> Score {
    let mut total = Score::default();
    for score in scores {
        total.reference_pages += score.reference_pages;
        total.candidate_pages += score.candidate_pages;
        total.documents += score.documents.max(1);
        total.pages_match +=
            if score.documents == 0 && score.reference_pages == score.candidate_pages {
                1
            } else {
                score.pages_match
            };
        total.reference_lines += score.reference_lines;
        total.same_lines += score.same_lines;
        total.pages_compared += score.pages_compared;
        total.page_breaks_agree += score.page_breaks_agree;
    }
    total.baseline = combine_stats(scores.iter().map(|s| &s.baseline));
    total.line_x = combine_stats(scores.iter().map(|s| &s.line_x));
    total.glyph_x = combine_stats(scores.iter().map(|s| &s.glyph_x));
    total.rules = combine_stats(scores.iter().map(|s| &s.rules));
    total.pictures = combine_stats(scores.iter().map(|s| &s.pictures));
    total
}

fn combine_stats<'a>(stats: impl Iterator<Item = &'a Stats>) -> Stats {
    let mut values = Vec::new();
    let mut fallback = Stats::default();
    let mut weighted = 0.0;
    for stat in stats {
        values.extend_from_slice(&stat.values);
        fallback.count += stat.count;
        weighted += stat.mean * stat.count as f64;
        fallback.max = fallback.max.max(stat.max);
        fallback.p95 = fallback.p95.max(stat.p95);
    }
    if values.len() == fallback.count {
        return Stats::of(values);
    }
    // older scores without their values: a conservative combination
    if fallback.count > 0 {
        fallback.mean = weighted / fallback.count as f64;
    }
    fallback
}

//! headers, footers and page numbers: text repeated near the top or the
//! bottom of many pages. they are taken out of the reading order and
//! reported on their own, so writers can make them real headers and
//! footers instead of repeating them in the body.

use std::collections::HashMap;

use super::fragments::Fragment;
use super::model::{RepeatedKind, RepeatedText};

/// the top and bottom part of a page, as a share of its height, where
/// headers and footers are looked for
const EDGE_SHARE: f32 = 0.1;
/// two pieces of text are in the same place if within this many points
const SAME_PLACE: f32 = 6.0;

/// one page's text, as the detector needs it.
pub(crate) struct PageFragments<'a> {
    pub page: usize,
    pub height: f32,
    pub fragments: &'a [Fragment],
}

/// what was found, and which fragments of each page it covers (page,
/// fragment index), so they can be left out of the body.
pub(crate) struct Repeated {
    pub texts: Vec<RepeatedText>,
    pub covered: Vec<(usize, usize)>,
}

struct Candidate {
    page: usize,
    index: usize,
    kind: RepeatedKind,
    text: String,
    y: f32,
}

pub(crate) fn find_repeated(pages: &[PageFragments]) -> Repeated {
    let candidates = edge_candidates(pages);
    let mut groups: HashMap<(u8, String), Vec<&Candidate>> = HashMap::new();
    for candidate in &candidates {
        let key = (candidate.kind as u8, normalised(&candidate.text));
        groups.entry(key).or_default().push(candidate);
    }
    let needed = (pages.len() as f32 * 0.5).ceil().max(2.0) as usize;
    let mut found = Repeated {
        texts: Vec::new(),
        covered: Vec::new(),
    };
    let mut keys: Vec<_> = groups.keys().cloned().collect();
    keys.sort();
    for key in keys {
        let group = same_place(&groups[&key]);
        let numbers = page_number_share(&group);
        let is_page_number = numbers >= 0.7;
        let (accepted, confidence) = if group.len() >= needed {
            let share = group.len() as f32 / pages.len().max(1) as f32;
            (true, (0.6 + 0.4 * share).min(0.95))
        } else if is_page_number && group.len() == 1 {
            // a lone number at the edge of a page that matches the page
            (true, 0.5)
        } else {
            (false, 0.0)
        };
        if !accepted {
            continue;
        }
        let first = group[0];
        let fragment = &pages
            .iter()
            .find(|p| p.page == first.page)
            .expect("candidate page exists")
            .fragments[first.index];
        found.texts.push(RepeatedText {
            kind: first.kind,
            text: first.text.clone(),
            page_number: is_page_number,
            pages: group.iter().map(|c| c.page).collect(),
            rect: fragment.rect,
            confidence,
        });
        found
            .covered
            .extend(group.iter().map(|c| (c.page, c.index)));
    }
    found
}

fn edge_candidates(pages: &[PageFragments]) -> Vec<Candidate> {
    let mut candidates = Vec::new();
    for page in pages {
        for (index, fragment) in page.fragments.iter().enumerate() {
            let kind = if fragment.rect.bottom() <= page.height * EDGE_SHARE {
                RepeatedKind::Header
            } else if fragment.rect.y >= page.height * (1.0 - EDGE_SHARE) {
                RepeatedKind::Footer
            } else {
                continue;
            };
            candidates.push(Candidate {
                page: page.page,
                index,
                kind,
                text: fragment.text(),
                y: fragment.rect.y,
            });
        }
    }
    candidates
}

/// the candidates that sit where the first one does, one per page.
fn same_place<'a>(group: &[&'a Candidate]) -> Vec<&'a Candidate> {
    let mut kept: Vec<&Candidate> = Vec::new();
    let Some(first) = group.first() else {
        return kept;
    };
    for candidate in group {
        let new_page = !kept.iter().any(|k| k.page == candidate.page);
        if new_page && (candidate.y - first.y).abs() <= SAME_PLACE {
            kept.push(candidate);
        }
    }
    kept
}

/// digits stand for any number, so "Page 3" and "Page 12" match.
fn normalised(text: &str) -> String {
    let mut out = String::new();
    let mut in_number = false;
    for c in text.trim().to_lowercase().chars() {
        if c.is_ascii_digit() {
            if !in_number {
                out.push('#');
            }
            in_number = true;
        } else {
            in_number = false;
            if !c.is_whitespace() || !out.ends_with(' ') {
                out.push(if c.is_whitespace() { ' ' } else { c });
            }
        }
    }
    out
}

/// the number a page number text shows: "7", "Page 7", "7 of 20", "- 7 -".
pub(crate) fn page_number_value(text: &str) -> Option<u32> {
    let lower = text.trim().to_lowercase();
    let lower = lower.trim_matches(|c: char| c == '-' || c == '–' || c.is_whitespace());
    let lower = lower.strip_prefix("page").map(str::trim).unwrap_or(lower);
    let first: String = lower.chars().take_while(char::is_ascii_digit).collect();
    let rest = lower[first.len()..].trim();
    let rest_ok = rest.is_empty()
        || rest
            .strip_prefix("of")
            .or_else(|| rest.strip_prefix('/'))
            .is_some_and(|total| {
                total.trim().chars().all(|c| c.is_ascii_digit()) && !total.trim().is_empty()
            });
    if first.is_empty() || !rest_ok {
        return None;
    }
    first.parse().ok()
}

/// how many of the group show a number that goes up with the page, by
/// the same offset (a document may start counting at a later page).
fn page_number_share(group: &[&Candidate]) -> f32 {
    let offsets: Vec<i64> = group
        .iter()
        .filter_map(|c| page_number_value(&c.text).map(|n| i64::from(n) - c.page as i64))
        .collect();
    if offsets.is_empty() {
        return 0.0;
    }
    let mut counts: HashMap<i64, usize> = HashMap::new();
    for offset in &offsets {
        *counts.entry(*offset).or_default() += 1;
    }
    let best = counts.values().max().copied().unwrap_or(0);
    // a lone number counts only if it is the page's own number
    if group.len() == 1 && offsets[0] != 1 {
        return 0.0;
    }
    best as f32 / group.len() as f32
}

#[cfg(test)]
mod tests {
    use super::super::fragments::tests::fragment;
    use super::*;

    #[test]
    fn page_numbers_are_read() {
        assert_eq!(page_number_value("7"), Some(7));
        assert_eq!(page_number_value("Page 7"), Some(7));
        assert_eq!(page_number_value("Page 7 of 20"), Some(7));
        assert_eq!(page_number_value("- 7 -"), Some(7));
        assert_eq!(page_number_value("7 apples"), None);
    }

    #[test]
    fn headers_and_numbers_repeat_across_pages() {
        let pages: Vec<Vec<Fragment>> = (0..3)
            .map(|page| {
                vec![
                    fragment("Biology notes", 72.0, 40.0, 9.0, false),
                    fragment("Body text here", 72.0, 300.0, 12.0, false),
                    fragment(&format!("{}", page + 1), 300.0, 760.0, 9.0, false),
                ]
            })
            .collect();
        let input: Vec<PageFragments> = pages
            .iter()
            .enumerate()
            .map(|(page, fragments)| PageFragments {
                page,
                height: 792.0,
                fragments,
            })
            .collect();
        let found = find_repeated(&input);
        assert_eq!(found.texts.len(), 2);
        let header = found
            .texts
            .iter()
            .find(|t| t.kind == RepeatedKind::Header)
            .unwrap();
        assert_eq!(header.text, "Biology notes");
        assert!(!header.page_number);
        assert_eq!(header.pages, vec![0, 1, 2]);
        let footer = found
            .texts
            .iter()
            .find(|t| t.kind == RepeatedKind::Footer)
            .unwrap();
        assert!(footer.page_number);
        assert_eq!(found.covered.len(), 6);
    }
}

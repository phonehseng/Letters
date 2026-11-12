//! the font searcher: a pure function from a query and the index to an
//! ordered list of families.
//!
//! typing is forgiving. each word must match the family somehow: its
//! name (with typos allowed, see fuzzy.rs), its designer, a kind of font
//! ("serif", "mono", "handwriting") or a writing system ("greek",
//! "vietnamese"). a few phrases are read as filters: "like Arial" or
//! "similar to Arial", "supports Vietnamese", "draw: ĐỒNG ₫".

use serde::{Deserialize, Serialize};

use crate::coverage::writing_named;
use crate::fuzzy::word_score;
use crate::index::{family_key, Family, FontIndex};
use crate::model::{Category, Source};
use crate::similar::similar_to;

/// where a family may come from, as the filter chips say it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Place {
    /// installed on the system
    OnThisComputer,
    /// added by the person
    AddedByMe,
    /// shipped with Letters or downloaded from the library
    LettersLibrary,
    /// in the online library, not downloaded yet
    ToDownload,
}

impl Place {
    fn holds(self, sources: &[Source]) -> bool {
        sources.iter().any(|source| match self {
            Place::OnThisComputer => *source == Source::System,
            Place::AddedByMe => *source == Source::Custom,
            Place::LettersLibrary => matches!(source, Source::Bundled | Source::Library),
            Place::ToDownload => *source == Source::Downloadable,
        })
    }
}

/// everything the search field and the filter chips ask for. every
/// filter is optional; an empty query lists everything.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct SearchQuery {
    pub text: String,
    /// any of these kinds (none means all)
    pub categories: Vec<Category>,
    /// every one of these writing system ids
    pub writing: Vec<String>,
    /// any of these places (none means all)
    pub places: Vec<Place>,
    pub monospace: bool,
    pub has_italic: bool,
    pub has_bold: bool,
    pub variable: bool,
    pub min_styles: usize,
    /// only fonts whose licence allows embedding
    pub embeddable: bool,
    /// only fonts that can draw every character of this text
    pub sample: String,
    pub similar_to: String,
    pub favourites_only: bool,
}

/// the person's own lists, which the search ranks and filters by.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Personal {
    pub favourites: Vec<String>,
    /// most recent first
    pub recent: Vec<String>,
}

impl Personal {
    pub fn is_favourite(&self, family: &str) -> bool {
        let key = family_key(family);
        self.favourites.iter().any(|name| family_key(name) == key)
    }
}

/// the answer: family positions in the index, best first.
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchResults {
    pub families: Vec<usize>,
    /// how the typed words were read, for the ui to say ("Similar to Arial")
    pub understood: Vec<String>,
    /// for a "similar to" search: which results have the same widths
    pub metric_compatible: Vec<usize>,
}

/// a query with its phrases taken out and turned into filters.
struct Reading {
    words: Vec<String>,
    query: SearchQuery,
    understood: Vec<String>,
}

fn read_phrases(query: &SearchQuery) -> Reading {
    let mut query = query.clone();
    let mut understood = Vec::new();
    let mut text = query.text.trim().to_string();
    let lower = text.to_ascii_lowercase();
    // "draw: ĐỒNG ₫" or "can it draw: ..." keeps the rest as sample text
    if let Some(at) = lower.find("draw:") {
        let sample = text[at + "draw:".len()..].trim().to_string();
        if !sample.is_empty() {
            understood.push(format!("Can draw “{sample}”"));
            query.sample = format!("{} {sample}", query.sample).trim().to_string();
        }
        text = text[..at].trim_end_matches("can it").trim().to_string();
    }
    let lower = text.to_ascii_lowercase();
    for prefix in ["similar to ", "like ", "looks like "] {
        if let Some(rest) = lower.strip_prefix(prefix) {
            query.similar_to = text.get(prefix.len()..).unwrap_or(rest).trim().to_string();
            text.clear();
            break;
        }
    }
    let lower = text.to_ascii_lowercase();
    for prefix in ["supports ", "for ", "in "] {
        if let Some(rest) = lower.strip_prefix(prefix) {
            if let Some(writing) = writing_named(rest) {
                query.writing.push(writing.id.to_string());
                understood.push(format!("Supports {}", writing.name));
                text.clear();
            }
            break;
        }
    }
    if !query.similar_to.is_empty() {
        understood.push(format!("Similar to {}", query.similar_to));
    }
    Reading {
        words: text.split_whitespace().map(str::to_string).collect(),
        query,
        understood,
    }
}

/// how well one typed word matches a family, 0 for not at all.
fn word_match(word: &str, family: &Family, writing: &[String]) -> u32 {
    let name = word_score(word, &family.name);
    if name > 0 {
        return name;
    }
    let lower = word.to_lowercase();
    if family.category().words().contains(&lower.as_str()) {
        return 200;
    }
    if let Some(found) = writing_named(&lower) {
        if writing.iter().any(|id| id == found.id) {
            return 200;
        }
    }
    let designer = family
        .catalogue
        .map(|entry| entry.designer.as_str())
        .unwrap_or("");
    if !designer.is_empty() && word.chars().count() >= 3 {
        let score = word_score(word, designer);
        if score >= 600 {
            return 150;
        }
    }
    0
}

fn passes_filters(
    family: &Family,
    query: &SearchQuery,
    personal: &Personal,
    writing: &[String],
) -> bool {
    let sources = family.sources();
    (query.categories.is_empty() || query.categories.contains(&family.category()))
        && query.writing.iter().all(|id| writing.contains(id))
        && (query.places.is_empty() || query.places.iter().any(|place| place.holds(&sources)))
        && (!query.monospace || family.monospace())
        && (!query.has_italic || family.has_italic())
        && (!query.has_bold || family.has_bold())
        && (!query.variable || family.variable())
        && family.style_count() >= query.min_styles
        && (!query.embeddable || family.embedding().allows_embedding())
        && (!query.favourites_only || personal.is_favourite(&family.name))
        && query
            .sample
            .chars()
            .filter(|c| !c.is_whitespace() && !c.is_control())
            .all(|c| family.can_draw(c))
}

/// searches the index. results are best first: for typed words by how
/// well they match; otherwise favourites, then fonts on this computer,
/// then the online library, each by name.
pub fn search(index: &FontIndex, query: &SearchQuery, personal: &Personal) -> SearchResults {
    let reading = read_phrases(query);
    let query = &reading.query;
    let similar = if query.similar_to.trim().is_empty() {
        None
    } else {
        Some(similar_to(index, &query.similar_to, 200))
    };
    let mut scored: Vec<(u32, usize)> = Vec::new();
    for (position, family) in index.families.iter().enumerate() {
        let writing = family.writing();
        if !passes_filters(family, query, personal, &writing) {
            continue;
        }
        let mut score = 0u32;
        let mut matched = true;
        for word in &reading.words {
            let found = word_match(word, family, &writing);
            if found == 0 {
                matched = false;
                break;
            }
            score += found;
        }
        if !matched {
            continue;
        }
        if let Some(similar) = &similar {
            let Some(rank) = similar
                .iter()
                .position(|entry| family_key(&entry.family) == family.key)
            else {
                continue;
            };
            score += 100_000 - rank as u32;
        }
        // small nudges that never beat a better name match
        if personal.is_favourite(&family.name) {
            score += 30;
        }
        if family.is_on_this_computer() {
            score += 20;
        }
        scored.push((score, position));
    }
    scored.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
    let families: Vec<usize> = scored.into_iter().map(|(_, position)| position).collect();
    let metric_compatible = similar
        .map(|similar| {
            families
                .iter()
                .copied()
                .filter(|position| {
                    similar.iter().any(|entry| {
                        entry.metric_compatible
                            && family_key(&entry.family) == index.families[*position].key
                    })
                })
                .collect()
        })
        .unwrap_or_default();
    SearchResults {
        families,
        understood: reading.understood,
        metric_compatible,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::Catalogue;
    use crate::index::tests::record;

    fn names(index: &FontIndex, results: &SearchResults) -> Vec<String> {
        results
            .families
            .iter()
            .map(|at| index.families[*at].name.clone())
            .collect()
    }

    fn text(text: &str) -> SearchQuery {
        SearchQuery {
            text: text.into(),
            ..SearchQuery::default()
        }
    }

    fn sample_index() -> FontIndex {
        let mut courier = record("Courier New", "Regular", 400, false, Source::System);
        courier.facts.category = Category::Monospace;
        courier.facts.monospace = true;
        FontIndex::build(
            vec![
                record("Helvetica", "Regular", 400, false, Source::System),
                record("Arial", "Regular", 400, false, Source::System),
                record("Arial", "Bold", 700, false, Source::System),
                courier,
                record("My Handwriting", "Regular", 400, false, Source::Custom),
            ],
            Catalogue::shipped(),
        )
    }

    #[test]
    fn prefixes_and_typos_find_names() {
        let index = sample_index();
        let personal = Personal::default();
        assert_eq!(
            names(&index, &search(&index, &text("helv"), &personal))[0],
            "Helvetica"
        );
        let garamond = names(&index, &search(&index, &text("garmond"), &personal));
        assert!(
            garamond.iter().take(5).any(|name| name == "EB Garamond"),
            "{garamond:?}"
        );
        assert_eq!(
            names(&index, &search(&index, &text("arail"), &personal))[0],
            "Arial"
        );
        // the exact name beats names that only start with it
        assert_eq!(
            names(&index, &search(&index, &text("Inter"), &personal))[0],
            "Inter"
        );
        assert!(search(&index, &text("zzqqxx"), &personal)
            .families
            .is_empty());
    }

    #[test]
    fn words_can_mean_kinds_and_languages() {
        let index = sample_index();
        let personal = Personal::default();
        let mono = search(
            &index,
            &SearchQuery {
                text: "mono".into(),
                places: vec![Place::OnThisComputer],
                ..SearchQuery::default()
            },
            &personal,
        );
        assert_eq!(names(&index, &mono), vec!["Courier New"]);
        let greek = search(&index, &text("supports greek"), &personal);
        assert_eq!(greek.understood, vec!["Supports Greek"]);
        assert!(greek
            .families
            .iter()
            .all(|at| index.families[*at].writing().contains(&"greek".into())));
        assert!(greek.families.len() > 50);
    }

    #[test]
    fn filters_narrow_by_place_and_properties() {
        let index = sample_index();
        let personal = Personal {
            favourites: vec!["Courier New".into()],
            recent: Vec::new(),
        };
        let mine = search(
            &index,
            &SearchQuery {
                places: vec![Place::AddedByMe],
                ..SearchQuery::default()
            },
            &personal,
        );
        assert_eq!(names(&index, &mine), vec!["My Handwriting"]);
        let bold_here = search(
            &index,
            &SearchQuery {
                has_bold: true,
                places: vec![Place::OnThisComputer],
                ..SearchQuery::default()
            },
            &personal,
        );
        assert_eq!(names(&index, &bold_here), vec!["Arial"]);
        let all = names(&index, &search(&index, &SearchQuery::default(), &personal));
        assert_eq!(all[0], "Courier New", "favourites first");
        assert!(all.len() > 1500);
        let download = search(
            &index,
            &SearchQuery {
                places: vec![Place::ToDownload],
                variable: true,
                ..SearchQuery::default()
            },
            &personal,
        );
        assert!(download.families.len() > 300);
    }

    #[test]
    fn sample_text_must_be_drawable() {
        let index = sample_index();
        let results = search(&index, &text("draw: ĐỒNG ₫"), &Personal::default());
        assert_eq!(results.understood, vec!["Can draw “ĐỒNG ₫”"]);
        let found = names(&index, &results);
        // the test faces only have ascii; library families with vietnamese qualify
        assert!(!found.contains(&"Arial".to_string()));
        assert!(found.contains(&"Inter".to_string()));
    }

    #[test]
    fn similar_to_reads_the_phrase() {
        let index = sample_index();
        let results = search(&index, &text("similar to Calibri"), &Personal::default());
        assert_eq!(results.understood, vec!["Similar to Calibri"]);
        assert_eq!(names(&index, &results)[0], "Carlito");
        assert_eq!(results.metric_compatible, vec![results.families[0]]);
    }
}

//! what a search is asked and what it answers: filters, the request,
//! and the results grouped by document. the interface receives these as json.

use serde::{Deserialize, Serialize};

use crate::model::{DocKind, DocNote};

/// narrowing a search, from the filter chips.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Filters {
    /// empty means every kind
    pub kinds: Vec<DocKind>,
    /// only documents inside this folder
    pub folder: Option<String>,
    /// milliseconds since 1970
    pub modified_after: Option<u64>,
    pub modified_before: Option<u64>,
    /// only this one document ("search this document")
    pub document: Option<String>,
    /// match words in titles and headings only
    pub titles_only: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct SearchRequest {
    pub query: String,
    pub filters: Filters,
    pub max_documents: usize,
    pub passages_per_document: usize,
    /// now, in milliseconds since 1970, for preferring recent documents
    pub now: u64,
}

impl Default for SearchRequest {
    fn default() -> Self {
        SearchRequest {
            query: String::new(),
            filters: Filters::default(),
            max_documents: 50,
            passages_per_document: 3,
            now: 0,
        }
    }
}

/// a word the person typed that was not found, and the one searched instead.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Correction {
    pub typed: String,
    pub used: String,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PassageHit {
    pub number: u32,
    pub page: u32,
    pub heading: String,
    pub snippet: String,
    /// [start, end) of each highlighted word in the snippet, counted in
    /// utf-16 units so the interface can slice the string directly
    pub highlights: Vec<[u32; 2]>,
    pub score: f32,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DocumentHit {
    pub path: String,
    pub title: String,
    pub kind: DocKind,
    pub modified: u64,
    pub pages: u32,
    pub notes: Vec<DocNote>,
    /// how many of its passages matched
    pub matching_passages: usize,
    pub passages: Vec<PassageHit>,
    pub score: f32,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchResults {
    pub documents: Vec<DocumentHit>,
    pub total_documents: usize,
    pub total_passages: usize,
    pub corrections: Vec<Correction>,
    pub micros: u64,
}

//! which pages of a document are scans, and which already have
//! recognised text: what the "Make This Document Searchable" banner and
//! the Scan tab's page choices are built on.

use letters_ocr::page_makeup;
use lopdf::Document;
use serde::Serialize;

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanOverview {
    pub page_count: u32,
    /// pages that are mostly a picture with little real text, from 0
    pub scanned: Vec<u32>,
    /// pages with invisible text from an earlier recognition (here or elsewhere)
    pub recognized: Vec<u32>,
    /// pages whose recognised text Letters wrote and can remove again
    pub with_layer: Vec<u32>,
}

impl ScanOverview {
    /// scanned pages nobody has read yet: what the banner offers to read.
    pub fn unread_scans(&self) -> Vec<u32> {
        self.scanned
            .iter()
            .copied()
            .filter(|page| !self.recognized.contains(page))
            .collect()
    }
}

/// looks at every page. a page that can't be read counts as neither.
pub fn scan_overview(doc: &Document) -> ScanOverview {
    let page_count = doc.get_pages().len() as u32;
    let mut overview = ScanOverview {
        page_count,
        ..ScanOverview::default()
    };
    for page in 0..page_count {
        let Ok(makeup) = page_makeup(doc, page) else {
            continue;
        };
        if makeup.is_scanned() {
            overview.scanned.push(page);
        }
        if makeup.has_recognised_text() {
            overview.recognized.push(page);
        }
        if makeup.has_ocr_layer {
            overview.with_layer.push(page);
        }
    }
    overview
}

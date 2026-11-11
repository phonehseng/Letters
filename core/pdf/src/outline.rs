//! the table of contents a pdf may carry (pdf calls it the outline,
//! most apps call the entries bookmarks).

/// one entry of the outline, flattened so the ui can show it as a list.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OutlineItem {
    pub title: String,
    /// zero based page the entry leads to
    pub page: u32,
    /// how deep the entry sits: 0 for top level entries
    pub level: u32,
}

/// a damaged outline can loop back on itself or nest without end.
/// these limits make sure reading it always finishes.
pub const MAX_OUTLINE_DEPTH: u32 = 32;
pub const MAX_OUTLINE_ITEMS: usize = 10_000;

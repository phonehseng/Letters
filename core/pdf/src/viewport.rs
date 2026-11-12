//! where the reader is looking in each open document, so the tiles on
//! screen are drawn before the ones a fast scroll left behind.
//!
//! the viewer tells the worker which pages are on screen, at which scale,
//! every time that changes. each message carries a generation number that
//! only goes up, so an older message arriving late never wins.

use std::collections::HashMap;

use crate::geometry::TileKey;
use crate::DocumentId;

/// a page this close to the screen is drawn soon, before the reader
/// scrolls onto it: one or two pages either side of what is shown
pub const NEAR_PAGES: u32 = 2;

/// the pages one view of a document shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Viewport {
    /// goes up with every change the viewer reports
    pub generation: u64,
    pub first_page: u32,
    pub last_page: u32,
    /// the scale the pages are drawn at; tiles at another scale (the
    /// thumbnails in the sidebar) belong to another view and are left alone
    pub scale_milli: u32,
}

/// how soon a waiting tile should be drawn.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Urgency {
    /// on screen now
    OnScreen,
    /// close to the screen, or for a view the worker isn't told about
    Soon,
    /// a page the reader has scrolled away from: not drawn at all
    LeftBehind,
}

/// the latest viewport of every document that has one.
#[derive(Default)]
pub struct Viewports {
    by_document: HashMap<DocumentId, Viewport>,
}

impl Viewports {
    /// keeps `viewport` unless a newer one is already known.
    pub fn set(&mut self, document: DocumentId, viewport: Viewport) {
        let is_newer = self
            .by_document
            .get(&document)
            .is_none_or(|known| viewport.generation > known.generation);
        if is_newer {
            self.by_document.insert(document, viewport);
        }
    }

    /// a closed document has no viewport.
    pub fn forget(&mut self, document: DocumentId) {
        self.by_document.remove(&document);
    }

    pub fn urgency(&self, key: &TileKey) -> Urgency {
        let Some(view) = self.by_document.get(&key.document) else {
            // nobody said where they are looking: every tile is wanted
            return Urgency::OnScreen;
        };
        if key.scale_milli != view.scale_milli {
            return Urgency::Soon;
        }
        if (view.first_page..=view.last_page).contains(&key.page) {
            return Urgency::OnScreen;
        }
        let near_start = view.first_page.saturating_sub(NEAR_PAGES);
        let near_end = view.last_page.saturating_add(NEAR_PAGES);
        if (near_start..=near_end).contains(&key.page) {
            Urgency::Soon
        } else {
            Urgency::LeftBehind
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DOCUMENT: DocumentId = DocumentId(1);

    fn tile(page: u32, scale_milli: u32) -> TileKey {
        TileKey {
            document: DOCUMENT,
            page,
            scale_milli,
            column: 0,
            row: 0,
        }
    }

    fn view(generation: u64, first_page: u32, last_page: u32) -> Viewport {
        Viewport {
            generation,
            first_page,
            last_page,
            scale_milli: 1000,
        }
    }

    #[test]
    fn without_a_viewport_every_tile_is_on_screen() {
        assert_eq!(
            Viewports::default().urgency(&tile(500, 1000)),
            Urgency::OnScreen
        );
    }

    #[test]
    fn pages_on_screen_come_first_then_their_neighbours_and_the_rest_are_left() {
        let mut viewports = Viewports::default();
        viewports.set(DOCUMENT, view(1, 10, 12));
        assert_eq!(viewports.urgency(&tile(11, 1000)), Urgency::OnScreen);
        assert_eq!(viewports.urgency(&tile(14, 1000)), Urgency::Soon);
        assert_eq!(viewports.urgency(&tile(8, 1000)), Urgency::Soon);
        assert_eq!(viewports.urgency(&tile(15, 1000)), Urgency::LeftBehind);
        assert_eq!(viewports.urgency(&tile(0, 1000)), Urgency::LeftBehind);
    }

    #[test]
    fn thumbnails_at_another_scale_are_never_left_behind() {
        let mut viewports = Viewports::default();
        viewports.set(DOCUMENT, view(1, 10, 12));
        assert_eq!(viewports.urgency(&tile(90, 150)), Urgency::Soon);
    }

    #[test]
    fn an_older_viewport_arriving_late_is_ignored() {
        let mut viewports = Viewports::default();
        viewports.set(DOCUMENT, view(5, 90, 91));
        viewports.set(DOCUMENT, view(4, 0, 1));
        assert_eq!(viewports.urgency(&tile(90, 1000)), Urgency::OnScreen);
    }

    #[test]
    fn a_closed_document_forgets_its_viewport() {
        let mut viewports = Viewports::default();
        viewports.set(DOCUMENT, view(1, 10, 12));
        viewports.forget(DOCUMENT);
        assert_eq!(viewports.urgency(&tile(0, 1000)), Urgency::OnScreen);
    }
}

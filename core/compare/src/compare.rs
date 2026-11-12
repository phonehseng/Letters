//! the whole comparison, from two page sources to one result.
//!
//! memory stays bounded however long the documents are:
//! 1. each page of both documents is read once and summarised by a small
//!    fingerprint; its letters are dropped straight away
//! 2. the fingerprints match pages (see page_align.rs)
//! 3. matched pages are compared one pair at a time. a pair's letters,
//!    words and pictures exist only while that pair is being compared
//! 4. removed and added passages are kept as word lists, which are small,
//!    so moves can be found across pages at the end
//!
//! progress is reported after every page, and the cancel flag is checked
//! before every page, so a long comparison can be stopped at any time.

use std::sync::atomic::{AtomicBool, Ordering};

use crate::fingerprint::PageFingerprint;
use crate::formatting::formatting_changes;
use crate::geometry::{PageSize, QuarterTurn, Rect};
use crate::layout::{compare_layout, page_size_change, LayoutInput};
use crate::moves::{find_moves, Passage};
use crate::options::{CompareOptions, Progress, Stage};
use crate::page_align::{align_pages, PageMatch};
use crate::pixel_diff::{changed_regions, PictureInput};
use crate::result::{Category, Change, ChangeKind, ComparisonResult, Offset, PagePair, Summary};
use crate::sentences;
use crate::source::PageSource;
use crate::text_diff::{compare_words, text_changes};
use crate::words::{read_page_text, PageText};
use crate::CompareError;

/// how many words of a page describe it when it was added or removed.
const OPENING_WORDS: usize = 8;

/// the most pixels one page picture may have for the picture comparison:
/// 8 million is a us letter page at 4 pixels per point, about 32 MB.
const MAX_PICTURE_PIXELS: f32 = 8_000_000.0;

/// compares two documents with the default settings and no progress reports.
pub fn compare_documents(
    left: &dyn PageSource,
    right: &dyn PageSource,
) -> Result<ComparisonResult, CompareError> {
    let never = AtomicBool::new(false);
    compare(left, right, &CompareOptions::default(), &mut |_| {}, &never)
}

/// compares the old version (`left`) with the new one (`right`).
///
/// `progress` is called after each page; setting `cancel` stops the
/// comparison before the next page with `CompareError::Cancelled`.
pub fn compare(
    left: &dyn PageSource,
    right: &dyn PageSource,
    options: &CompareOptions,
    progress: &mut dyn FnMut(Progress),
    cancel: &AtomicBool,
) -> Result<ComparisonResult, CompareError> {
    let mut work = Work {
        left,
        right,
        options,
        progress,
        cancel,
    };
    let (left_sizes, left_prints) = work.read_side(left, Stage::ReadingOld)?;
    let (right_sizes, right_prints) = work.read_side(right, Stage::ReadingNew)?;
    let matches = align_pages(&left_prints, &right_prints);
    drop((left_prints, right_prints));

    let mut pairs: Vec<PagePair> = Vec::with_capacity(matches.len());
    let mut passages = Passages::default();
    let mut ignored = 0;
    let total = matches.len() as u32;
    for (done, found) in matches.iter().enumerate() {
        work.check_cancel()?;
        let pair = match *found {
            PageMatch::Pair {
                left: old,
                right: new,
                similarity,
                moved,
            } => {
                let sizes = (left_sizes[old as usize], right_sizes[new as usize]);
                let compared = work.compare_pair(
                    (old, new),
                    sizes,
                    similarity,
                    moved,
                    pairs.len(),
                    &mut passages,
                )?;
                ignored += compared.1;
                compared.0
            }
            PageMatch::Removed { left: old } => {
                work.lone_page(Side::Old, old, left_sizes[old as usize])?
            }
            PageMatch::Added { right: new } => {
                work.lone_page(Side::New, new, right_sizes[new as usize])?
            }
        };
        pairs.push(pair);
        (work.progress)(Progress {
            stage: Stage::ComparingPages,
            done: done as u32 + 1,
            total,
        });
    }
    apply_moves(&mut pairs, &passages, options.min_move_words);
    for pair in &mut pairs {
        sort_changes(pair);
    }
    number_changes(&mut pairs);
    Ok(ComparisonResult {
        left_page_count: left.page_count(),
        right_page_count: right.page_count(),
        summary: Summary::of(&pairs, ignored),
        notes: overall_notes(left, right, options),
        page_pairs: pairs,
    })
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Side {
    Old,
    New,
}

/// removed and added passages from every pair, kept to find moves.
#[derive(Default)]
struct Passages {
    removed: Vec<Passage>,
    added: Vec<Passage>,
}

/// what every step of one comparison needs.
struct Work<'a, 'p> {
    left: &'a dyn PageSource,
    right: &'a dyn PageSource,
    options: &'a CompareOptions,
    progress: &'p mut dyn FnMut(Progress),
    cancel: &'a AtomicBool,
}

impl Work<'_, '_> {
    fn check_cancel(&self) -> Result<(), CompareError> {
        if self.cancel.load(Ordering::Relaxed) {
            Err(CompareError::Cancelled)
        } else {
            Ok(())
        }
    }

    fn page_text(
        &self,
        source: &dyn PageSource,
        page: u32,
        size: PageSize,
    ) -> Result<PageText, CompareError> {
        let glyphs = source.glyphs(page)?;
        Ok(read_page_text(&glyphs, size, self.options.ignore_case))
    }

    /// reads every page of one document into page sizes and fingerprints.
    fn read_side(
        &mut self,
        source: &dyn PageSource,
        stage: Stage,
    ) -> Result<(Vec<PageSize>, Vec<PageFingerprint>), CompareError> {
        let count = source.page_count();
        let mut sizes = Vec::with_capacity(count as usize);
        let mut prints = Vec::with_capacity(count as usize);
        for page in 0..count {
            self.check_cancel()?;
            let size = source.page_size(page)?;
            let text = self.page_text(source, page, size)?;
            prints.push(PageFingerprint::of_words(
                text.words.iter().map(|word| word.key.as_str()),
            ));
            sizes.push(size);
            (self.progress)(Progress {
                stage,
                done: page + 1,
                total: count,
            });
        }
        Ok((sizes, prints))
    }

    /// a page only one document has.
    fn lone_page(&self, side: Side, page: u32, size: PageSize) -> Result<PagePair, CompareError> {
        let source = if side == Side::Old {
            self.left
        } else {
            self.right
        };
        let opening = self
            .page_text(source, page, size)?
            .opening_words(OPENING_WORDS);
        let began = if opening.is_empty() {
            String::new()
        } else {
            format!(" (it begins {})", sentences::quote(&opening))
        };
        let (kind, detail) = match side {
            Side::Old => (
                ChangeKind::PageRemoved,
                format!("Old page {} was removed{began}", page + 1),
            ),
            Side::New => (
                ChangeKind::PageAdded,
                format!("Page {} was added{began}", page + 1),
            ),
        };
        let change = Change {
            kind,
            category: Category::Page,
            left_page: (side == Side::Old).then_some(page),
            right_page: (side == Side::New).then_some(page),
            left_rects: if side == Side::Old {
                vec![size.full_page()]
            } else {
                Vec::new()
            },
            right_rects: if side == Side::New {
                vec![size.full_page()]
            } else {
                Vec::new()
            },
            old_text: (side == Side::Old && !opening.is_empty()).then(|| opening.clone()),
            new_text: (side == Side::New && !opening.is_empty()).then_some(opening),
            detail,
            ..Change::default()
        };
        Ok(PagePair {
            left_page: (side == Side::Old).then_some(page),
            right_page: (side == Side::New).then_some(page),
            left_size: (side == Side::Old).then_some(size),
            right_size: (side == Side::New).then_some(size),
            changes: vec![change],
            ..PagePair::default()
        })
    }

    /// compares one old page with the new page it became. returns the pair
    /// and how many spacing-only differences were left out.
    fn compare_pair(
        &self,
        pages: (u32, u32),
        sizes: (PageSize, PageSize),
        similarity: f32,
        moved: bool,
        pair_index: usize,
        passages: &mut Passages,
    ) -> Result<(PagePair, u32), CompareError> {
        let options = self.options;
        let old = self.page_text(self.left, pages.0, sizes.0)?;
        let new = self.page_text(self.right, pages.1, sizes.1)?;
        let mut changes = Vec::new();
        if moved {
            changes.push(page_moved(pages, sizes));
        }
        let resized = page_size_change(sizes.0, sizes.1, pages);
        let turned = old.turn != new.turn;
        if options.compare_layout {
            changes.extend(resized.clone());
        }

        let pieces = compare_words(&old, &new);
        let text = text_changes(&pieces, &old, &new, pages, !options.ignore_spacing);
        let base = changes.len();
        for (index, keys) in text.removed {
            passages.removed.push(Passage {
                pair: pair_index,
                change: base + index,
                keys,
            });
        }
        for (index, keys) in text.added {
            passages.added.push(Passage {
                pair: pair_index,
                change: base + index,
                keys,
            });
        }
        changes.extend(text.changes);

        let (format_changes, restyled) = if options.compare_formatting {
            formatting_changes(&pieces, &old, &new, pages)
        } else {
            (Vec::new(), vec![false; old.words.len()])
        };
        changes.extend(format_changes);

        let layout = compare_layout(&LayoutInput {
            pieces: &pieces,
            left: &old,
            right: &new,
            restyled: &restyled,
            pages,
            tolerance: options.position_tolerance,
            page_resized: resized.is_some() || turned,
            report: options.compare_layout,
        });
        changes.extend(layout.changes);

        // the offset only lines up pictures of pages that aren't turned
        let offset = if old.turn == QuarterTurn::None && new.turn == QuarterTurn::None {
            layout.offset
        } else {
            Offset::default()
        };
        let mut notes = textless_notes(&old, &new, pages);
        if options.compare_pictures {
            changes.extend(self.picture_changes(pages, &old, &new, layout.offset, &mut notes));
        }
        let pair = PagePair {
            left_page: Some(pages.0),
            right_page: Some(pages.1),
            left_size: Some(sizes.0),
            right_size: Some(sizes.1),
            similarity,
            changes,
            scroll_map: layout.scroll_map,
            offset,
            notes,
        };
        Ok((pair, text.ignored))
    }

    /// draws both pages and reports the areas whose pictures differ.
    fn picture_changes(
        &self,
        pages: (u32, u32),
        old: &PageText,
        new: &PageText,
        offset: Offset,
        notes: &mut Vec<String>,
    ) -> Vec<Change> {
        if !self.left.can_render() || !self.right.can_render() {
            return Vec::new();
        }
        let fingerprints = (
            self.left.drawing_fingerprint(pages.0),
            self.right.drawing_fingerprint(pages.1),
        );
        if let (Some(a), Some(b)) = fingerprints {
            if a == b {
                // the pages draw exactly the same things
                return Vec::new();
            }
        }
        let scale = picture_scale(self.options.picture_scale, old.size, new.size);
        let drawn = self
            .left
            .render(pages.0, scale)
            .and_then(|a| self.right.render(pages.1, scale).map(|b| (a, b)));
        let (old_picture, new_picture) = match drawn {
            Ok(pictures) => pictures,
            Err(error) => {
                notes.push(format!("Pictures on this page weren’t compared. {error}"));
                return Vec::new();
            }
        };
        let old_picture = old_picture.turned_counter_clockwise(old.turn.degrees() / 90);
        let new_picture = new_picture.turned_counter_clockwise(new.turn.degrees() / 90);
        let regions = changed_regions(&PictureInput {
            left: &old_picture,
            right: &new_picture,
            left_mask: &upright_word_rects(old),
            right_mask: &upright_word_rects(new),
            offset: if old.has_text() && new.has_text() {
                (offset.dx, offset.dy)
            } else {
                (0.0, 0.0)
            },
            scale,
        });
        let shift = if old.has_text() && new.has_text() {
            offset
        } else {
            Offset::default()
        };
        regions
            .into_iter()
            .map(|region| Change {
                kind: ChangeKind::Redrawn,
                category: Category::Image,
                left_page: Some(pages.0),
                right_page: Some(pages.1),
                left_rects: vec![old.to_visible(&region)],
                right_rects: vec![new.to_visible(&region.shifted(shift.dx, shift.dy))],
                detail: "Picture or drawing changed in this area".to_owned(),
                ..Change::default()
            })
            .collect()
    }
}

/// the scale pages are drawn at for the picture comparison: the one asked
/// for, but never so large that one picture passes `MAX_PICTURE_PIXELS`.
/// a poster-sized page is drawn coarser instead of using gigabytes.
fn picture_scale(wanted: f32, old: PageSize, new: PageSize) -> f32 {
    let largest_area = (old.width * old.height)
        .max(new.width * new.height)
        .max(1.0);
    let limit = (MAX_PICTURE_PIXELS / largest_area).sqrt();
    wanted.clamp(0.1, 4.0).min(limit)
}

/// every word's area in the upright frame, both parts of a hyphenated word included.
fn upright_word_rects(page: &PageText) -> Vec<Rect> {
    let upright = page.upright_size();
    page.words
        .iter()
        .flat_map(|word| word.rects.iter())
        .map(|rect| page.turn.rect_to_upright(rect, page.size))
        .filter(|rect| rect.x < upright.width && rect.y < upright.height)
        .collect()
}

fn page_moved(pages: (u32, u32), sizes: (PageSize, PageSize)) -> Change {
    Change {
        kind: ChangeKind::PageMoved,
        category: Category::Page,
        left_page: Some(pages.0),
        right_page: Some(pages.1),
        left_rects: vec![sizes.0.full_page()],
        right_rects: vec![sizes.1.full_page()],
        detail: format!("Old page {} is now page {}", pages.0 + 1, pages.1 + 1),
        ..Change::default()
    }
}

fn textless_notes(old: &PageText, new: &PageText, pages: (u32, u32)) -> Vec<String> {
    let mut notes = Vec::new();
    if !old.has_text() {
        notes.push(format!(
            "Old page {} has no text that can be read (it may be a scan), so it was compared as a picture.",
            pages.0 + 1
        ));
    }
    if !new.has_text() {
        notes.push(format!(
            "New page {} has no text that can be read (it may be a scan), so it was compared as a picture.",
            pages.1 + 1
        ));
    }
    notes
}

fn overall_notes(
    left: &dyn PageSource,
    right: &dyn PageSource,
    options: &CompareOptions,
) -> Vec<String> {
    let mut notes = Vec::new();
    if options.compare_pictures && !(left.can_render() && right.can_render()) {
        notes.push(
            "Pictures and drawings weren’t compared, because the pages can’t be drawn here."
                .to_owned(),
        );
    }
    if options.ignore_spacing {
        notes.push("Differences only in spacing or hyphenation are not listed.".to_owned());
    }
    notes
}

/// turns removed-and-added passage pairs into single moved changes.
fn apply_moves(pairs: &mut [PagePair], passages: &Passages, min_words: usize) {
    let moves = find_moves(&passages.removed, &passages.added, min_words);
    let mut absorbed: Vec<(usize, usize)> = Vec::new();
    for found in moves {
        let from = &passages.removed[found.removed];
        let to = &passages.added[found.added];
        let addition = pairs[to.pair].changes[to.change].clone();
        let change = &mut pairs[from.pair].changes[from.change];
        change.kind = ChangeKind::Moved;
        change.right_page = addition.right_page;
        change.right_rects = addition.right_rects;
        change.new_text = addition.new_text;
        change.detail = sentences::moved(
            change.old_text.as_deref().unwrap_or_default(),
            change.left_page.unwrap_or(0),
            change.right_page.unwrap_or(0),
            from.pair == to.pair,
            found.similarity < 1.0,
        );
        absorbed.push((to.pair, to.change));
    }
    // remove from the back so earlier positions stay valid
    absorbed.sort_unstable_by(|a, b| b.cmp(a));
    for (pair, change) in absorbed {
        pairs[pair].changes.remove(change);
    }
}

/// page-wide changes first, then the rest in the order they appear on the page.
fn sort_changes(pair: &mut PagePair) {
    let right_page = pair.right_page;
    let key = |change: &Change| {
        let page_wide = matches!(
            change.kind,
            ChangeKind::PageMoved
                | ChangeKind::PageAdded
                | ChangeKind::PageRemoved
                | ChangeKind::Resized
        );
        let on_right = change.right_page == right_page && !change.right_rects.is_empty();
        let rect = if on_right {
            change.right_rects.first()
        } else {
            change.left_rects.first()
        };
        let (y, x) = rect.map(|rect| (rect.y, rect.x)).unwrap_or((0.0, 0.0));
        (!page_wide, y, x)
    };
    pair.changes.sort_by(|a, b| {
        let (a, b) = (key(a), key(b));
        a.0.cmp(&b.0)
            .then(a.1.total_cmp(&b.1))
            .then(a.2.total_cmp(&b.2))
    });
}

/// gives every change an id, counting through the pairs in order.
fn number_changes(pairs: &mut [PagePair]) {
    let changes = pairs.iter_mut().flat_map(|pair| pair.changes.iter_mut());
    for (id, change) in changes.enumerate() {
        change.id = id as u32;
    }
}

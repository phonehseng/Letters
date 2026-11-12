//! reading order: does the order of the tags roughly follow the order a
//! sighted reader would take through the page?
//!
//! each page is split into blocks (paragraphs, headings, lists, tables,
//! figures: the tags just below any grouping tag). the order the tags give
//! them is compared with the order the layout suggests (see `layout`).
//! only strong disagreements are reported, because unusual layouts can be
//! right in an unusual order.

use std::collections::HashMap;

use crate::geometry::Rect;
use crate::layout::{broken_steps, reading_order};
use crate::objects::ObjectRef;
use crate::report::{FixKind, Location};
use crate::rules::{Collector, Context};
use crate::structure::{Kid, StructTree};

/// the share of steps between blocks that may jump around before a page
/// is reported. a third leaves room for a sidebar or a caption read out
/// of place, which is often what the author meant.
const ALLOWED_BROKEN_STEPS: f32 = 0.34;

/// one block on a page.
pub(crate) struct PageBlock {
    pub element: usize,
    pub rect: Rect,
}

pub(crate) fn check(ctx: &Context, out: &mut Collector) {
    let (true, Some(tree)) = (ctx.tagged, &ctx.tree) else {
        super::skip_untagged(out, &["reading-order-mismatch"]);
        return;
    };
    let mut compared_any = false;
    for page in 0..ctx.page_count() {
        let blocks = page_blocks(ctx, tree, page);
        if blocks.len() < 3 {
            continue;
        }
        compared_any = true;
        let visual = visual_order(&blocks);
        let tag_order: Vec<usize> = (0..blocks.len()).collect();
        let (share, broken) = broken_steps(&tag_order, &visual);
        if share > ALLOWED_BROKEN_STEPS && broken >= 2 {
            let proposal: Vec<ObjectRef> = visual
                .iter()
                .filter_map(|&b| tree.elements[blocks[b].element].id.map(ObjectRef::from))
                .collect();
            out.report(
                "reading-order-mismatch",
                format!(
                    "On page {}, {broken} of the {} steps from one block to the next jump ahead or back compared with the layout.",
                    page + 1,
                    tag_order.len() - 1
                ),
                Location::page(page, None),
                Some(FixKind::SetReadingOrder { page, proposal }),
            );
        }
    }
    if !compared_any {
        out.not_applicable(
            "reading-order-mismatch",
            "No page has enough separate blocks to compare orders.",
        );
    }
}

/// the blocks with content on a page, in the order the tags give them.
pub(crate) fn page_blocks(ctx: &Context, tree: &StructTree, page: u32) -> Vec<PageBlock> {
    let page_id = ctx.pages[page as usize].id;
    let mut order: Vec<usize> = Vec::new();
    let mut rects: HashMap<usize, Rect> = HashMap::new();
    for index in tree.preorder() {
        for kid in &tree.elements[index].kids {
            let Kid::Content {
                page: Some(kid_page),
                mcid,
                in_stream: false,
            } = kid
            else {
                continue;
            };
            if *kid_page != page_id {
                continue;
            }
            let Some(rect) = ctx.mcid_rects.get(&(page, *mcid)) else {
                continue;
            };
            let block = tree.block_of(index);
            match rects.get_mut(&block) {
                Some(sum) => *sum = sum.union(rect),
                None => {
                    rects.insert(block, *rect);
                    order.push(block);
                }
            }
        }
    }
    order
        .into_iter()
        .map(|element| PageBlock {
            element,
            rect: rects[&element],
        })
        .collect()
}

/// the layout's order of the blocks, as indexes into `blocks`.
pub(crate) fn visual_order(blocks: &[PageBlock]) -> Vec<usize> {
    let rects: Vec<Rect> = blocks.iter().map(|block| block.rect).collect();
    reading_order(&rects)
}

//! the steps every newer content edit shares: check the document may be
//! edited, read the page, work out the changes in a draft, write them,
//! commit only if all of it worked, and find the edited objects again.

use lopdf::Document;

use crate::draft::Draft;
use crate::error::EditError;
use crate::geometry::{Point, Rect};
use crate::layout::{self, PageModel};
use crate::model::PageObjects;
use crate::rewrite::{self, Plan};
use crate::structure;
use crate::summary::{ChangesSummary, ContentEditResult, EditConsent};
use crate::text_fonts::FontNotice;

/// where an edited object should be afterwards, to give back its new id.
#[derive(Clone, Debug)]
pub(crate) enum Landing {
    Path(Rect),
    Image([Point; 4]),
    /// the text block whose box starts at this point (top left)
    Block(Point),
}

/// what the work of an edit hands back besides its plan.
#[derive(Default)]
pub(crate) struct Outcome {
    pub summary: ChangesSummary,
    pub notices: Vec<FontNotice>,
    pub landings: Vec<Landing>,
}

/// runs an edit of one page: `work` fills the plan and the draft.
pub(crate) fn run_edit(
    doc: &mut Document,
    page: usize,
    consent: &EditConsent,
    work: impl FnOnce(&PageModel, &mut Draft, &mut Plan) -> Result<Outcome, EditError>,
) -> Result<ContentEditResult, EditError> {
    structure::guard(doc, consent)?;
    let model = layout::build(doc, page)?;
    let changes;
    let mut outcome;
    {
        let mut draft = Draft::new(doc);
        let mut plan = Plan::default();
        outcome = work(&model, &mut draft, &mut plan)?;
        rewrite::apply(&mut draft, &model.run, plan)?;
        changes = draft.finish();
    }
    changes.commit(doc);
    outcome.summary.page(page);
    let objects = layout::build(doc, page)?.objects();
    let object_ids = find_landings(&objects, &outcome.landings);
    Ok(ContentEditResult {
        objects,
        notices: outcome.notices,
        summary: outcome.summary,
        object_ids,
    })
}

const LANDING_TOLERANCE: f32 = 0.25;

fn close(a: f32, b: f32) -> bool {
    (a - b).abs() < LANDING_TOLERANCE
}

fn same_rect(a: &Rect, b: &Rect) -> bool {
    close(a.x, b.x)
        && close(a.y, b.y)
        && close(a.right(), b.right())
        && close(a.bottom(), b.bottom())
}

/// the ids of the objects that landed where the edit put them.
pub(crate) fn find_landings(objects: &PageObjects, landings: &[Landing]) -> Vec<String> {
    let mut taken: Vec<String> = Vec::new();
    for landing in landings {
        let found = match landing {
            Landing::Path(rect) => objects
                .paths
                .iter()
                .find(|path| same_rect(&path.rect, rect) && !taken.contains(&path.id))
                .map(|path| path.id.clone()),
            Landing::Image(quad) => objects
                .images
                .iter()
                .find(|image| {
                    image
                        .quad
                        .iter()
                        .zip(quad)
                        .all(|(a, b)| close(a.x, b.x) && close(a.y, b.y))
                        && !taken.contains(&image.id)
                })
                .map(|image| image.id.clone()),
            Landing::Block(corner) => objects
                .text_blocks
                .iter()
                .filter(|block| !taken.contains(&block.id))
                .min_by(|a, b| {
                    let distance = |rect: &Rect| Point::new(rect.x, rect.y).minus(*corner).length();
                    distance(&a.rect).total_cmp(&distance(&b.rect))
                })
                .map(|block| block.id.clone()),
        };
        if let Some(id) = found {
            taken.push(id);
        }
    }
    taken
}

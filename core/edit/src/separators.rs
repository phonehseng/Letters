//! horizontal rules that separate text: two lines with a rule between
//! them never join one block, so each cell of a ruled table is a block of
//! its own even when its rows are evenly spaced like a paragraph.

use crate::geometry::Point;
use crate::interpret::{segment_points, Interpretation};
use crate::layout::LineModel;

/// a drawn rule this thin (points) can separate lines; anything taller is
/// a box that text may sit inside.
const RULE_THICKNESS: f32 = 3.0;
/// text running more steeply than this isn't checked: rules between
/// turned lines are rare and the test would need their own direction.
const UPRIGHT: f32 = 0.995;
/// underlines are drawn up to about a fifth of the size below the
/// baseline; a rule must lie lower than this to separate lines.
const UNDERLINE_DEPTH: f32 = 0.3;

/// a horizontal rule on screen.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Separator {
    pub y: f32,
    pub left: f32,
    pub right: f32,
}

/// every path that is a thin horizontal rule on screen.
pub(crate) fn horizontal_separators(run: &Interpretation) -> Vec<Separator> {
    run.paths
        .iter()
        .filter(|path| !path.paint.shading && (path.paint.fill || path.paint.stroke))
        .filter_map(|path| {
            let points: Vec<Point> = segment_points(&path.segments)
                .into_iter()
                .map(|point| run.space.visible_point(path.ctm.apply(point)))
                .collect();
            let top = points.iter().map(|p| p.y).fold(f32::MAX, f32::min);
            let bottom = points.iter().map(|p| p.y).fold(f32::MIN, f32::max);
            let left = points.iter().map(|p| p.x).fold(f32::MAX, f32::min);
            let right = points.iter().map(|p| p.x).fold(f32::MIN, f32::max);
            let thin = bottom - top <= RULE_THICKNESS && right - left > bottom - top;
            thin.then_some(Separator {
                y: (top + bottom) / 2.0,
                left,
                right,
            })
        })
        .collect()
}

/// true when a rule runs between two lines, across where both are.
pub(crate) fn separated(separators: &[Separator], above: &LineModel, below: &LineModel) -> bool {
    if above.direction.x < UPRIGHT || below.direction.x < UPRIGHT {
        return false;
    }
    // underlines sit just under the baseline; a rule between rows sits lower
    let from = above.start.y + UNDERLINE_DEPTH * above.size;
    let to = below.start.y - below.size;
    let left = above.start.x.max(below.start.x);
    let right = (above.start.x + above.length).min(below.start.x + below.length);
    separators
        .iter()
        .any(|rule| rule.y > from && rule.y < to && rule.left < right && rule.right > left)
}

//! where a paragraph goes vertically: the exact line height and the twips
//! of its first line's top whose printed baselines are nearest the source's.
//!
//! line tops inside a paragraph add up in twips and each follows the frame
//! rule (recipe 6.1 and 6.6) with the measured baseline table of its line
//! height: 393 of 393 first and second lines, 390 of 393 third lines on
//! the predicted pixel for heights 200 to 330 twips (`tests/hybrid_probe.rs`).

use super::candidates::PageGeometry;
use super::rules::on_pixel_edge;
use super::PlannedLine;
use crate::calibration::model::{is_landscape, Axis, WordModel};
use crate::plan::placement::{baseline_pixel, BASELINE_SHARE};
use crate::units::round_half_even;
use crate::word::sizes::line_twips_for;
use crate::word::writer_model::{exported_y, page_pixels, StepTable};

/// line heights tried around a paragraph's measured pitch: the source's
/// pitch is rarely a whole twip, and a neighbouring height can put more
/// baselines on their nearest pixel.
pub const LINE_HEIGHT_SPREAD_TWIPS: i64 = 3;
/// frame tops tried around the first guess (recipe 7.3 searches the same).
const TOP_SEARCH_TWIPS: i64 = 12;

/// the exact line heights a paragraph with this pitch may get.
pub fn line_height_choices(pitch_pt: f64) -> Vec<i64> {
    let wanted = round_half_even(pitch_pt * 20.0) as i64;
    (wanted - LINE_HEIGHT_SPREAD_TWIPS..=wanted + LINE_HEIGHT_SPREAD_TWIPS).collect()
}

/// what one placement is judged by, smallest first.
#[derive(Clone, Copy, Debug, PartialEq, PartialOrd)]
struct Score {
    /// the worst line's distance to its source baseline, thousandths of a point
    worst_milli: f64,
    /// line tops on a pixel edge
    edges: usize,
    /// twips from the first guess
    from_guess: i64,
}

/// the worst baseline error of a paragraph placed at `top`. on a pixel edge
/// word may take the pixel above or below (`rules::on_pixel_edge`), so such
/// a line must pass whichever it takes.
fn score(
    model: &WordModel,
    y_table: &StepTable,
    height_px: i64,
    sources: &[f64],
    line_twips: i64,
    top: i64,
) -> Score {
    let mut worst: f64 = 0.0;
    let mut edges = 0;
    for (k, source) in sources.iter().enumerate() {
        let line_top = top + k as i64 * line_twips;
        let pixel = baseline_pixel(model, line_twips, line_top);
        let spread = i64::from(on_pixel_edge(line_top));
        edges += spread as usize;
        for candidate in pixel - spread..=pixel + spread {
            let error = (exported_y(candidate, y_table, height_px) - source).abs();
            worst = worst.max(error);
        }
    }
    Score {
        worst_milli: (worst * 1000.0).round(),
        edges,
        from_guess: 0,
    }
}

/// where the lines of a paragraph go, and how far the worst one prints
/// from its source baseline.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Placement {
    pub line_twips: i64,
    pub top_twips: i64,
    pub worst_pt: f64,
}

/// a paragraph whose lines can't all print within this of their source
/// baselines is cut into pieces that can: with the x lattice floor of up to
/// 0.072 pt (recipe 11) the glyphs then stay within 0.11 pt, under the
/// 0.125 pt target. a pixel off (0.12 pt) is never accepted this way.
pub const VERTICAL_FIT_PT: f64 = 0.08;

/// the placement of a paragraph's lines.
pub fn vertical_placement(
    members: &[&PlannedLine],
    geometry: &PageGeometry,
    model: &mut WordModel,
    pitch: f64,
) -> Placement {
    let height_px = page_pixels(geometry.height_pt);
    let landscape = is_landscape(geometry.width_pt, geometry.height_pt);
    let y_table = model.table(Axis::Y, height_px, landscape);
    let sources: Vec<f64> = members.iter().map(|l| l.line.y()).collect();
    let choices = if members.len() == 1 {
        vec![line_twips_for(members[0].line.max_size())]
    } else {
        line_height_choices(pitch)
    };
    let first_top_pt = geometry.height_pt - sources[0];
    let mut best: Option<(Score, i64, i64)> = None;
    for line_twips in choices {
        let baseline_pt = BASELINE_SHARE * line_twips as f64 / 20.0;
        let guess = round_half_even((first_top_pt - baseline_pt) * 20.0) as i64;
        for top in guess - TOP_SEARCH_TWIPS..=guess + TOP_SEARCH_TWIPS {
            let mut candidate = score(model, &y_table, height_px, &sources, line_twips, top);
            candidate.from_guess = (top - guess).abs();
            if best.is_none_or(|(b, _, _)| candidate < b) {
                best = Some((candidate, line_twips, top));
            }
        }
    }
    let (score, line_twips, top_twips) = best.expect("there is always a line height to try");
    Placement {
        line_twips,
        top_twips,
        worst_pt: score.worst_milli / 1000.0,
    }
}

/// cuts a paragraph's lines into runs that each fit one placement within
/// `VERTICAL_FIT_PT`, greedily from the top: the source's line pitch can
/// follow a pattern of pixels no single exact line height reproduces.
pub fn vertical_pieces(
    lines: &[usize],
    planned: &[PlannedLine],
    geometry: &PageGeometry,
    model: &mut WordModel,
    pitch: f64,
) -> Vec<Vec<usize>> {
    let mut pieces: Vec<Vec<usize>> = Vec::new();
    let mut current: Vec<usize> = Vec::new();
    for &line in lines {
        current.push(line);
        let members: Vec<&PlannedLine> = current.iter().map(|&i| &planned[i]).collect();
        let fits = vertical_placement(&members, geometry, model, pitch).worst_pt <= VERTICAL_FIT_PT;
        if !fits && current.len() > 1 {
            current.pop();
            pieces.push(std::mem::replace(&mut current, vec![line]));
        }
    }
    pieces.push(current);
    pieces
}

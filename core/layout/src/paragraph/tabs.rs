//! finding the tab stop a tab character goes to.
//!
//! word measures stops from the text margin, not from the indent. custom
//! stops come first; default stops (every `default_tab`, half an inch
//! unless the document says otherwise) continue only to the right of the
//! last custom stop; and a hanging indent adds an implicit stop at the
//! left indent, which is how a list label's tab reaches the text.

use letters_office::model::{TabKind, TabLeader};

use crate::style::{ParaStyle, Tab};

/// a tab character's destination.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Stop {
    pub position: f64,
    pub kind: TabKind,
    pub leader: TabLeader,
}

/// the stop a tab at `pen` (points from the text margin) goes to.
pub fn next_stop(style: &ParaStyle, pen: f64, first_line: bool) -> Stop {
    // a stop exactly at the pen does not count: the tab must move on
    const EPSILON: f64 = 0.01;
    let custom = style
        .tabs
        .iter()
        .filter(|tab| tab.kind != TabKind::Bar)
        .find(|tab| tab.position > pen + EPSILON);
    let hanging =
        (first_line && style.first_line < 0.0 && style.left > pen + EPSILON).then_some(style.left);
    match (custom, hanging) {
        (Some(tab), Some(left)) if left < tab.position => left_stop(left),
        (Some(tab), _) => stop_of(tab),
        (None, Some(left)) => left_stop(left),
        (None, None) => {
            let last_custom = style
                .tabs
                .iter()
                .map(|tab| tab.position)
                .fold(f64::MIN, f64::max);
            left_stop(next_default(pen.max(last_custom), style.default_tab))
        }
    }
}

fn stop_of(tab: &Tab) -> Stop {
    Stop {
        position: tab.position,
        kind: tab.kind,
        leader: tab.leader,
    }
}

fn left_stop(position: f64) -> Stop {
    Stop {
        position,
        kind: TabKind::Left,
        leader: TabLeader::None,
    }
}

/// the first multiple of `step` beyond `pen`.
fn next_default(pen: f64, step: f64) -> f64 {
    let step = step.max(1.0);
    let count = (pen / step).floor() + 1.0;
    // a pen sitting (almost) exactly on a default stop moves to the next
    let mut position = count * step;
    if position - pen < 0.01 {
        position += step;
    }
    position.max(step)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::flavor::Flavor;
    use letters_office::model::{Document, Paragraph};

    fn style() -> ParaStyle {
        crate::style::para_style(&Document::new(), &Paragraph::with_text(""), Flavor::Word)
    }

    #[test]
    fn default_stops_every_half_inch() {
        let style = style();
        assert_eq!(next_stop(&style, 0.0, true).position, 36.0);
        assert_eq!(next_stop(&style, 35.0, true).position, 36.0);
        assert_eq!(next_stop(&style, 36.0, true).position, 72.0);
        assert_eq!(next_stop(&style, 40.0, false).position, 72.0);
    }

    #[test]
    fn default_stops_start_after_the_last_custom_stop() {
        let mut style = style();
        style.tabs = vec![Tab {
            position: 100.0,
            kind: TabKind::Right,
            leader: TabLeader::Dot,
        }];
        let stop = next_stop(&style, 10.0, true);
        assert_eq!((stop.position, stop.kind), (100.0, TabKind::Right));
        // past the custom stop, defaults resume after it: 108, not 36
        assert_eq!(next_stop(&style, 101.0, true).position, 108.0);
    }

    #[test]
    fn hanging_indent_is_an_implicit_stop() {
        let mut style = style();
        style.left = 36.0;
        style.first_line = -18.0;
        assert_eq!(next_stop(&style, 25.0, true).position, 36.0);
        // not on later lines, and not once the pen is past it
        assert_eq!(next_stop(&style, 25.0, false).position, 36.0);
        assert_eq!(next_stop(&style, 40.0, true).position, 72.0);
    }
}

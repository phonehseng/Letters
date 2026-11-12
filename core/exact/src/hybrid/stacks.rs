//! anchoring: how promoted paragraphs are held on the page so that an edit
//! moves only what it should.
//!
//! consecutive promoted paragraphs of one column go into one frame: word
//! puts paragraphs with the same frame properties into the same frame and
//! stacks them, so a paragraph that grows by a line pushes the paragraphs
//! below it in its frame down by exactly that line, and nothing else moves.
//! the gaps between them are written as space before (word keeps the
//! larger of space after and space before, `rules::paragraph_gap`), so the
//! twips of every line top are exactly the ones each paragraph was proven
//! at on its own (`tests/hybrid_probe.rs`: off pixel edges 135 of 135
//! following lines and 64 of 70 first lines on the predicted pixel).
//!
//! measured and rejected: body flow (paragraphs in the page's text area,
//! margins for the column) put 75 of 234 lines a pixel lower than the
//! frame rule, against 25 of 234 in stacked frames (almost all on pixel
//! edges), and it can only hold one column per section.
//!
//! a positioned line (a paragraph left in round one's form) ends a stack,
//! because only adjacent paragraphs share a frame.

use serde::Serialize;

use super::candidates::{Candidate, FrameBox};

/// one item of a page in document order.
#[derive(Clone, Debug)]
pub enum Item<'a> {
    Paragraph {
        candidate: &'a Candidate,
        /// this paragraph may not share a frame with the one above it
        own_frame: bool,
    },
    /// a positioned line, already as xml
    Locked(&'a str),
}

/// one frame of stacked paragraphs.
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StackReport {
    pub page: usize,
    pub paragraphs: usize,
    pub lines: usize,
    /// the first planned line of each paragraph in the frame, top to bottom
    pub members: Vec<usize>,
}

struct Stack<'a> {
    frame: FrameBox,
    members: Vec<(&'a Candidate, i64)>,
    /// twips of the line top after the last member's last line
    bottom: i64,
}

impl<'a> Stack<'a> {
    fn start(candidate: &'a Candidate) -> Self {
        let lines = candidate.lines.len() as i64;
        Self {
            frame: candidate.own_frame(),
            members: vec![(candidate, 0)],
            bottom: candidate.top_twips + lines * candidate.line_twips,
        }
    }

    /// a paragraph joins when it starts below the stack's last line and
    /// inside its column.
    fn try_join(&mut self, candidate: &'a Candidate) -> bool {
        let own = candidate.own_frame();
        let before = candidate.top_twips - self.bottom;
        let inside = own.left_px >= self.frame.left_px && own.left_px < self.frame.right_px;
        if before < 0 || !inside {
            return false;
        }
        self.frame.right_px = self.frame.right_px.max(own.right_px);
        self.members.push((candidate, before));
        self.bottom = candidate.top_twips + candidate.lines.len() as i64 * candidate.line_twips;
        true
    }

    fn xml(&self) -> String {
        self.members
            .iter()
            .map(|(candidate, before)| candidate.xml(&self.frame, *before))
            .collect()
    }
}

/// the page's xml items, with consecutive promoted paragraphs of a column
/// stacked into shared frames.
pub fn arrange(page: usize, items: &[Item]) -> (Vec<String>, Vec<StackReport>) {
    let mut xml = Vec::new();
    let mut reports = Vec::new();
    let mut current: Option<Stack> = None;
    let close = |stack: Option<Stack>, xml: &mut Vec<String>, reports: &mut Vec<StackReport>| {
        if let Some(stack) = stack {
            xml.push(stack.xml());
            reports.push(StackReport {
                page,
                paragraphs: stack.members.len(),
                lines: stack.members.iter().map(|(c, _)| c.lines.len()).sum(),
                members: stack.members.iter().map(|(c, _)| c.lines[0]).collect(),
            });
        }
    };
    for item in items {
        match item {
            Item::Paragraph {
                candidate,
                own_frame,
            } => {
                let joined = !own_frame
                    && current
                        .as_mut()
                        .is_some_and(|stack| stack.try_join(candidate));
                if !joined {
                    close(current.take(), &mut xml, &mut reports);
                    current = Some(Stack::start(candidate));
                }
            }
            Item::Locked(line) => {
                close(current.take(), &mut xml, &mut reports);
                xml.push(line.to_string());
            }
        }
    }
    close(current.take(), &mut xml, &mut reports);
    (xml, reports)
}

/// every paragraph in its own frame, exactly where it was proven (the
/// proof exports use this, so candidates can't move each other).
pub fn arrange_apart(items: &[Item]) -> Vec<String> {
    items
        .iter()
        .map(|item| match item {
            Item::Paragraph { candidate, .. } => candidate.xml(&candidate.own_frame(), 0),
            Item::Locked(line) => line.to_string(),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::super::candidates::Level;
    use super::*;
    use crate::plan::glyphs::CharRun;

    fn candidate(top: i64, lines: usize, left: i64) -> Candidate {
        Candidate {
            page: 0,
            lines: (0..lines).collect(),
            level: Level::EveryGlyph,
            justified: false,
            first_start_px: left,
            body_start_px: left,
            right_px: left + 2000,
            window_px: (left + 1990, left + 2010),
            top_twips: top,
            line_twips: 240,
            pins: Vec::new(),
            runs: vec![CharRun {
                char: "a".into(),
                family: "F".into(),
                half_points: 20,
                color: "000000".into(),
                spacing_twips: 0,
                hidden: false,
            }],
            breaks_after: Vec::new(),
        }
    }

    #[test]
    fn paragraphs_of_a_column_share_one_frame_with_space_before() {
        let a = candidate(1000, 3, 600);
        let b = candidate(1000 + 720 + 120, 2, 600);
        let items = [
            Item::Paragraph {
                candidate: &a,
                own_frame: false,
            },
            Item::Paragraph {
                candidate: &b,
                own_frame: false,
            },
        ];
        let (xml, stacks) = arrange(0, &items);
        assert_eq!(xml.len(), 1);
        assert_eq!(stacks[0].paragraphs, 2);
        assert_eq!(xml[0].matches(r#"w:y="1000""#).count(), 2);
        assert!(xml[0].contains(r#"w:before="120""#));
    }

    #[test]
    fn overlap_another_column_or_a_positioned_line_starts_a_new_frame() {
        let a = candidate(1000, 3, 600);
        let overlapping = candidate(1500, 2, 600);
        let other_column = candidate(2000, 2, 3000);
        let after = candidate(3000, 2, 3000);
        let items = [
            Item::Paragraph {
                candidate: &a,
                own_frame: false,
            },
            Item::Paragraph {
                candidate: &overlapping,
                own_frame: false,
            },
            Item::Paragraph {
                candidate: &other_column,
                own_frame: false,
            },
            Item::Locked("<w:p/>"),
            Item::Paragraph {
                candidate: &after,
                own_frame: false,
            },
        ];
        let (xml, stacks) = arrange(0, &items);
        assert_eq!(stacks.len(), 4);
        assert_eq!(xml.len(), 5);
    }
}

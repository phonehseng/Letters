//! text runs from the structure analysis turned into the office model's
//! inline text with marks.
//!
//! the body font and size live in the Normal style, so a run only carries
//! a mark where it differs from ordinary body text. that keeps the result
//! easy to restyle: changing Normal changes the whole document.

use letters_convert::{Rgb, TextRun};
use letters_office::model::{Inline, LinkTarget, Mark};

/// what ordinary body text looks like in this document.
#[derive(Clone, Debug, PartialEq)]
pub struct BodyText {
    pub family: String,
    /// in half points, as the office model stores sizes
    pub half_points: u32,
}

/// how a run should be written in a given kind of block.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RunContext {
    Body,
    /// a heading's style already makes it big and bold
    Heading,
}

/// pdf sizes are fractional points; word stores whole half points.
pub fn half_points(size: f32) -> u32 {
    (size * 2.0).round().clamp(2.0, 3276.0) as u32
}

/// the runs of one paragraph as inline content, neighbours with the same
/// formatting joined.
pub fn inlines(runs: &[TextRun], body: &BodyText, context: RunContext) -> Vec<Inline> {
    let pieces = runs
        .iter()
        .filter(|run| !run.text.is_empty())
        .map(|run| Inline::styled(&run.text, marks_for(run, body, context)))
        .collect();
    letters_office::model::merge_adjacent_text(pieces)
}

/// the marks a run needs on top of the paragraph's style.
pub fn marks_for(run: &TextRun, body: &BodyText, context: RunContext) -> Vec<Mark> {
    let mut marks = Vec::new();
    if run.font_family != body.family && !run.font_family.is_empty() {
        marks.push(Mark::FontFamily {
            name: run.font_family.clone(),
            theme: None,
            extra: Vec::new(),
        });
    }
    if run.bold && context == RunContext::Body {
        marks.push(Mark::Bold);
    }
    if run.italic {
        marks.push(Mark::Italic);
    }
    if let Some(hex) = colour_mark(run.color) {
        marks.push(Mark::Color { hex });
    }
    let size = half_points(run.size);
    // a superscript is smaller on purpose; word shrinks it itself
    if context == RunContext::Body && size != body.half_points && !run.superscript {
        marks.push(Mark::FontSize { half_points: size });
    }
    if run.superscript {
        marks.push(Mark::Superscript);
    }
    if let Some(link) = run.link.as_deref().and_then(web_link) {
        marks.push(Mark::Link(link));
    }
    letters_office::model::normalize_marks(&mut marks);
    marks
}

/// black is the default colour, so it needs no mark.
fn colour_mark(color: Rgb) -> Option<String> {
    (color != Rgb::BLACK).then(|| color.hex())
}

/// only links that lead out of the document are kept as links. a link to
/// another page of the pdf ("#page=4") has no page to point at once the
/// text flows, so its words stay plain text.
fn web_link(target: &str) -> Option<LinkTarget> {
    let outside = ["http://", "https://", "mailto:"]
        .iter()
        .any(|scheme| target.starts_with(scheme));
    outside.then(|| LinkTarget {
        href: Some(target.to_string()),
        anchor: None,
        tooltip: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use letters_convert::GenericFamily;

    fn run(text: &str) -> TextRun {
        TextRun {
            text: text.into(),
            font_family: "Times New Roman".into(),
            generic_family: GenericFamily::Serif,
            bold: false,
            italic: false,
            size: 12.0,
            color: Rgb::BLACK,
            link: None,
            superscript: false,
            font_ref: String::new(),
        }
    }

    fn body() -> BodyText {
        BodyText {
            family: "Times New Roman".into(),
            half_points: 24,
        }
    }

    #[test]
    fn plain_body_text_needs_no_marks() {
        assert!(marks_for(&run("hello"), &body(), RunContext::Body).is_empty());
    }

    #[test]
    fn differences_from_the_body_become_marks() {
        let mut bold_red = run("x");
        bold_red.bold = true;
        bold_red.color = Rgb(200, 0, 0);
        bold_red.size = 14.0;
        bold_red.font_family = "Arial".into();
        let marks = marks_for(&bold_red, &body(), RunContext::Body);
        assert!(marks.contains(&Mark::Bold));
        assert!(marks.contains(&Mark::Color {
            hex: "C80000".into()
        }));
        assert!(marks.contains(&Mark::FontSize { half_points: 28 }));
        assert!(matches!(&marks[0], Mark::FontFamily { name, .. } if name == "Arial"));
    }

    #[test]
    fn headings_leave_size_and_weight_to_their_style() {
        let mut big = run("Title");
        big.bold = true;
        big.size = 20.0;
        assert!(marks_for(&big, &body(), RunContext::Heading).is_empty());
    }

    #[test]
    fn only_outside_links_stay_links() {
        let mut web = run("site");
        web.link = Some("https://example.com".into());
        let mut page = run("see page 4");
        page.link = Some("#page=4".into());
        assert!(marks_for(&web, &body(), RunContext::Body)
            .iter()
            .any(|mark| matches!(mark, Mark::Link(target) if target.href.as_deref() == Some("https://example.com"))));
        assert!(marks_for(&page, &body(), RunContext::Body).is_empty());
    }

    #[test]
    fn runs_with_the_same_look_are_joined() {
        let joined = inlines(&[run("one "), run("two")], &body(), RunContext::Body);
        assert_eq!(joined, vec![Inline::text("one two")]);
    }
}

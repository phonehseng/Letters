//! sticky notes (/Text), their pop-up windows, and replies.
//!
//! a sticky note is a small icon on the page whose comment opens in a
//! pop-up. a reply is another /Text annotation that points at the one it
//! answers with /IRT ("in reply to"); viewers gather replies into a
//! thread under the original. replies to any kind of mark work the same
//! way, so a highlight can have a conversation too.

use lopdf::{Dictionary, Document, Object, ObjectId};
use serde::{Deserialize, Serialize};

use crate::annots::{self, PageContext};
use crate::appearance::{Appearance, Canvas};
use crate::color::Color;
use crate::common::{self, Basics};
use crate::error::AnnotateError;
use crate::fonts::StandardFont;
use crate::geometry::{Point, Rect};
use crate::page_space::PageSpace;
use crate::pdf_values::{dict_name, dict_rect, name, number_array};
use crate::read::AnnotationInfo;
use crate::winansi;

/// the size of a note's icon on the page, in points. readers draw their
/// own note icons at about this size, so ours match.
pub(crate) const ICON_SIZE: f32 = 20.0;

/// the size of the pop-up window that shows a note's comment.
const POPUP_WIDTH: f32 = 200.0;
const POPUP_HEIGHT: f32 = 120.0;

/// the icons the pdf standard names for sticky notes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum NoteIcon {
    #[default]
    Comment,
    Note,
    Help,
    Key,
    Insert,
    Paragraph,
    NewParagraph,
}

impl NoteIcon {
    pub(crate) fn pdf_name(self) -> &'static str {
        match self {
            NoteIcon::Comment => "Comment",
            NoteIcon::Note => "Note",
            NoteIcon::Help => "Help",
            NoteIcon::Key => "Key",
            NoteIcon::Insert => "Insert",
            NoteIcon::Paragraph => "Paragraph",
            NoteIcon::NewParagraph => "NewParagraph",
        }
    }

    /// reads /Name. icons other apps invented are shown as a note.
    pub(crate) fn from_pdf_name(icon: &[u8]) -> NoteIcon {
        match icon {
            b"Comment" => NoteIcon::Comment,
            b"Help" => NoteIcon::Help,
            b"Key" => NoteIcon::Key,
            b"Insert" => NoteIcon::Insert,
            b"Paragraph" => NoteIcon::Paragraph,
            b"NewParagraph" => NoteIcon::NewParagraph,
            _ => NoteIcon::Note,
        }
    }
}

fn default_note_color() -> Color {
    Color::HIGHLIGHT_YELLOW
}

/// what the app sends to put a sticky note on a page.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NoteParams {
    /// where the icon's top left corner goes, in visible page points
    pub position: Point,
    #[serde(default)]
    pub icon: NoteIcon,
    #[serde(default = "default_note_color")]
    pub color: Color,
    /// the comment
    pub contents: String,
    #[serde(default)]
    pub author: Option<String>,
    #[serde(default = "crate::full_opacity")]
    pub opacity: f32,
    /// whether the pop-up starts open in viewers that honour it
    #[serde(default)]
    pub open: bool,
}

/// what the app sends to answer a note or any other mark.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReplyParams {
    pub contents: String,
    #[serde(default)]
    pub author: Option<String>,
}

/// puts a sticky note on a page, with a pop-up for its comment.
pub fn add_note(
    doc: &mut Document,
    page: usize,
    params: &NoteParams,
) -> Result<AnnotationInfo, AnnotateError> {
    let page = annots::page(doc, page)?;
    common::check_point(params.position)?;
    let basics = Basics {
        color: Some(params.color),
        opacity: params.opacity,
        contents: Some(&params.contents),
        author: params.author.as_deref(),
    };
    let icon = icon_rect(params.position, &page.space);
    let mut annotation = common::new_dictionary("Text", &page, &basics);
    annotation.set("Name", name(params.icon.pdf_name()));
    annotation.set("Open", Object::Boolean(params.open));
    annotation.set("Rect", number_array(&page.space.user_rect(icon)));
    let info = common::add(doc, &page, annotation)?;
    let note = annots::find(doc, page.id, &info.id)?;
    add_popup(doc, &page, note, icon, params.open)?;
    crate::read::info(doc, &page, note)
}

/// answers an annotation. the reply is listed with `reply_to` set to the
/// original's id, and draws nothing on the page itself: viewers show
/// replies inside the original's thread, not as icons of their own.
pub fn add_reply(
    doc: &mut Document,
    page: usize,
    reply_to: &str,
    params: &ReplyParams,
) -> Result<AnnotationInfo, AnnotateError> {
    let page = annots::page(doc, page)?;
    annots::make_entries_objects(doc, page.id)?;
    let original_id = annots::find(doc, page.id, reply_to)?;
    let original = doc
        .get_dictionary(original_id)
        .map_err(|_| AnnotateError::UnknownAnnotation)?;
    let basics = Basics {
        color: common::color(doc, original, b"C"),
        opacity: 1.0,
        contents: Some(&params.contents),
        author: params.author.as_deref(),
    };
    // the reply sits on the original so viewers that place replies by
    // position put it in the right thread
    let rect = dict_rect(doc, original, b"Rect").unwrap_or([0.0; 4]);
    let mut reply = common::new_dictionary("Text", &page, &basics);
    reply.set("Name", name(NoteIcon::Comment.pdf_name()));
    reply.set("IRT", Object::Reference(original_id));
    reply.set("RT", name("R"));
    reply.set("Rect", number_array(&rect));
    common::add(doc, &page, reply)
}

/// the icon's square, kept on the page so it can always be clicked.
fn icon_rect(position: Point, space: &PageSpace) -> Rect {
    let x = position.x.clamp(0.0, (space.width - ICON_SIZE).max(0.0));
    let y = position.y.clamp(0.0, (space.height - ICON_SIZE).max(0.0));
    Rect::new(x, y, ICON_SIZE, ICON_SIZE)
}

/// adds the pop-up window for a note, beside its icon where it fits.
fn add_popup(
    doc: &mut Document,
    page: &PageContext,
    note: ObjectId,
    icon: Rect,
    open: bool,
) -> Result<(), AnnotateError> {
    let space = &page.space;
    let right_side = icon.right() + 4.0;
    let x = if right_side + POPUP_WIDTH <= space.width {
        right_side
    } else {
        (icon.x - 4.0 - POPUP_WIDTH).max(0.0)
    };
    let y = icon.y.min((space.height - POPUP_HEIGHT).max(0.0));
    let rect = Rect::new(x, y, POPUP_WIDTH, POPUP_HEIGHT);

    let mut popup = Dictionary::new();
    popup.set("Type", name("Annot"));
    popup.set("Subtype", name("Popup"));
    popup.set("Parent", Object::Reference(note));
    popup.set("P", Object::Reference(page.id));
    popup.set("Rect", number_array(&space.user_rect(rect)));
    popup.set("Open", Object::Boolean(open));
    let popup_id = doc.add_object(popup);
    annots::append(doc, page.id, popup_id)?;
    doc.get_dictionary_mut(note)
        .map_err(|_| AnnotateError::UnknownAnnotation)?
        .set("Popup", Object::Reference(popup_id));
    Ok(())
}

/// draws the note's icon, or nothing for a reply.
pub(crate) fn draw(doc: &Document, annotation: &Dictionary, space: &PageSpace) -> Appearance {
    let frame = dict_rect(doc, annotation, b"Rect")
        .map(|rect| space.visible_rect(rect))
        .filter(|rect| !rect.is_empty())
        .unwrap_or(Rect::new(0.0, 0.0, ICON_SIZE, ICON_SIZE));
    let mut canvas = Canvas::new(frame);
    if annotation.has(b"IRT") {
        return canvas.finish();
    }
    let icon = dict_name(doc, annotation, b"Name")
        .map(NoteIcon::from_pdf_name)
        .unwrap_or(NoteIcon::Note);
    let color = common::color(doc, annotation, b"C").unwrap_or(Color::HIGHLIGHT_YELLOW);
    canvas.opacity(common::opacity(doc, annotation), false);
    draw_icon(&mut canvas, icon, color, frame);
    canvas.finish()
}

/// a darker shade of the note's colour for outlines, so a pale note
/// still has a visible edge on white paper.
fn outline_color(color: Color) -> Color {
    let darken = |channel: u8| (f32::from(channel) * 0.45) as u8;
    Color::new(darken(color.r), darken(color.g), darken(color.b))
}

/// the icons are designed on a 20 by 20 grid with y going down and
/// scaled to the note's square.
fn draw_icon(canvas: &mut Canvas, icon: NoteIcon, color: Color, frame: Rect) {
    let scale = frame.width.min(frame.height) / ICON_SIZE;
    let at = |x: f32, y: f32| Point::new(frame.x + x * scale, frame.y + y * scale);
    canvas.fill_color(color);
    canvas.stroke_color(outline_color(color));
    canvas.line_width(scale);
    canvas.round_line_ends();
    match icon {
        NoteIcon::Comment => {
            // a speech bubble with a tail and two lines of "text"
            let outline = [
                at(2.0, 3.0),
                at(18.0, 3.0),
                at(18.0, 14.0),
                at(10.0, 14.0),
                at(5.0, 18.0),
                at(6.0, 14.0),
                at(2.0, 14.0),
            ];
            canvas.polyline(&outline, true);
            canvas.fill_and_stroke();
            draw_lines(
                canvas,
                &[
                    (at(5.0, 7.0), at(15.0, 7.0)),
                    (at(5.0, 10.5), at(12.0, 10.5)),
                ],
            );
        }
        NoteIcon::Note => {
            // a sheet of paper with a folded corner
            let outline = [
                at(4.0, 2.0),
                at(12.5, 2.0),
                at(16.0, 5.5),
                at(16.0, 18.0),
                at(4.0, 18.0),
            ];
            canvas.polyline(&outline, true);
            canvas.fill_and_stroke();
            draw_lines(
                canvas,
                &[
                    (at(6.5, 8.0), at(13.5, 8.0)),
                    (at(6.5, 11.0), at(13.5, 11.0)),
                    (at(6.5, 14.0), at(11.0, 14.0)),
                ],
            );
        }
        NoteIcon::Help => {
            canvas.ellipse(Rect::new(
                at(2.0, 2.0).x,
                at(2.0, 2.0).y,
                16.0 * scale,
                16.0 * scale,
            ));
            canvas.fill_and_stroke();
            draw_symbol(canvas, "?", at(6.6, 15.0), 13.0 * scale, color);
        }
        NoteIcon::Key => {
            canvas.ellipse(Rect::new(
                at(2.0, 5.5).x,
                at(2.0, 5.5).y,
                9.0 * scale,
                9.0 * scale,
            ));
            canvas.fill_and_stroke();
            canvas.line_width(2.0 * scale);
            draw_lines(
                canvas,
                &[
                    (at(11.0, 10.0), at(18.0, 10.0)),
                    (at(15.0, 10.0), at(15.0, 13.0)),
                    (at(17.5, 10.0), at(17.5, 13.0)),
                ],
            );
        }
        NoteIcon::Insert => {
            // a caret, the proofreader's mark for "insert here"
            let caret = [
                at(2.0, 17.0),
                at(10.0, 3.0),
                at(18.0, 17.0),
                at(14.0, 17.0),
                at(10.0, 10.0),
                at(6.0, 17.0),
            ];
            canvas.polyline(&caret, true);
            canvas.fill_and_stroke();
        }
        NoteIcon::Paragraph | NoteIcon::NewParagraph => {
            canvas.rect(Rect::new(
                at(2.0, 2.0).x,
                at(2.0, 2.0).y,
                16.0 * scale,
                16.0 * scale,
            ));
            canvas.fill_and_stroke();
            let mark = if icon == NoteIcon::Paragraph {
                "\u{b6}"
            } else {
                "NP"
            };
            let start = if icon == NoteIcon::Paragraph {
                at(6.0, 15.0)
            } else {
                at(3.6, 14.0)
            };
            draw_symbol(canvas, mark, start, 11.0 * scale, color);
        }
    }
}

fn draw_lines(canvas: &mut Canvas, lines: &[(Point, Point)]) {
    for (start, end) in lines {
        canvas.move_to(*start);
        canvas.line_to(*end);
    }
    canvas.stroke();
}

/// a character drawn in bold in the icon, dark on the note's colour.
fn draw_symbol(canvas: &mut Canvas, symbol: &str, baseline_start: Point, size: f32, color: Color) {
    let encoded = winansi::encode(symbol).expect("icon symbols are WinAnsi characters");
    canvas.fill_color(outline_color(color));
    canvas.text(StandardFont::HelveticaBold, size, baseline_start, &encoded);
}

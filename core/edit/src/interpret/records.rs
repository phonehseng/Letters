//! what running a page's content finds: the content streams, fonts, text
//! show operators, glyphs, images, paths and marked-content sequences,
//! each with the operator that drew it so an edit can find it again.

use lopdf::{Dictionary, ObjectId};

use crate::colors::Color;
use crate::fonts::pdf_font::PdfFont;
use crate::geometry::{Matrix, Point};
use crate::lexer::Operation;
use crate::page_space::PageSpace;

/// where a content stream comes from.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) enum Origin {
    /// the page's own /Contents, by position in the list
    Page { index: usize },
    /// a form XObject, drawn by operator `op` (named `name`) of `parent`
    Form {
        parent: usize,
        op: usize,
        name: Vec<u8>,
    },
}

/// one content stream that was run.
#[derive(Clone, Debug)]
pub(crate) struct Source {
    pub origin: Origin,
    pub object: ObjectId,
    pub content: Vec<u8>,
    pub operations: Vec<Operation>,
    /// the resources this stream's names refer to: a dictionary object, or
    /// none for a page whose resources are written in (or inherited by) it
    pub resources: Resources,
    /// a short fingerprint of the content, to notice stale ids
    pub fingerprint: u32,
    /// the matrix in effect when the stream starts (page space)
    pub base_ctm: Matrix,
}

/// where a stream's resource dictionary lives.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Resources {
    /// the page's own /Resources (possibly inherited)
    Page,
    /// a /Resources dictionary object
    Object(ObjectId),
    /// a dictionary written inside a form's stream dictionary
    InForm(ObjectId),
}

/// the text state when a show operator starts, which an edit needs to
/// write text that continues seamlessly.
#[derive(Clone, Debug)]
pub(crate) struct TextState {
    pub text_matrix: Matrix,
    pub line_matrix: Matrix,
    pub font_name: Vec<u8>,
    pub font: Option<usize>,
    pub size: f32,
    pub char_spacing: f32,
    pub word_spacing: f32,
    /// horizontal scaling as a fraction (Tz 100 is 1.0)
    pub h_scale: f32,
    pub leading: f32,
    pub rise: f32,
    pub render_mode: i64,
    pub fill: FillColor,
}

/// the fill colour and the operators that set it, to set it back exactly.
/// the same shape is used for the stroke colour.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct FillColor {
    pub color: Color,
    /// the colour-space operator ("/Cs1 cs"), when one was used
    pub space_operator: Option<Vec<u8>>,
    pub color_operator: Option<Vec<u8>>,
    /// the stream the operators came from, since named colour spaces only
    /// mean something there
    pub source: Option<usize>,
    /// the colour space's name as written ("DeviceRGB", "Cs1", "Pattern")
    pub space: String,
    /// the numbers given to the colour operator
    pub components: Vec<f32>,
    /// the pattern's resource name, when the colour is a pattern
    pub pattern: Option<Vec<u8>>,
    /// the space is a pattern space (named /Pattern or an array that
    /// starts with /Pattern)
    pub pattern_space: bool,
    /// a Separation or DeviceN space: one number is an amount of ink, so
    /// 1 looks dark, the opposite of a grey
    pub ink_tint: bool,
}

impl FillColor {
    pub(super) fn black() -> Self {
        FillColor {
            color: Color::BLACK,
            space_operator: None,
            color_operator: None,
            source: None,
            space: "DeviceGray".into(),
            components: vec![0.0],
            pattern: None,
            pattern_space: false,
            ink_tint: false,
        }
    }
}

/// how lines are stroked: width, dash, caps and joins.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct LineStyle {
    /// in the user space of the path
    pub width: f32,
    pub dash: Vec<f32>,
    pub dash_phase: f32,
    pub cap: i64,
    pub join: i64,
    pub miter: f32,
}

/// one piece of a path, in the user space it was written in.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Segment {
    Move(Point),
    Line(Point),
    Curve(Point, Point, Point),
    Close,
    /// x, y, width, height as written by "re"
    Rect(Point, f32, f32),
}

/// how a path was painted.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct Paint {
    pub fill: bool,
    pub stroke: bool,
    pub even_odd: bool,
    /// the path also became (part of) the clipping path
    pub clip: bool,
    /// a shading painted with "sh" (filling the clip)
    pub shading: bool,
}

/// one drawn path: construction operators then a painting operator.
#[derive(Clone, Debug)]
pub(crate) struct PathRecord {
    pub source: usize,
    /// the first construction operator (for a shading, the first operator
    /// of the clip path that limits it, when it has one)
    pub first_op: usize,
    /// the painting operator ("f", "S", "B", "n", "sh" ...)
    pub paint_op: usize,
    pub segments: Vec<Segment>,
    pub paint: Paint,
    pub ctm: Matrix,
    pub fill: FillColor,
    pub stroke: FillColor,
    pub line: LineStyle,
    pub fill_alpha: f32,
    pub stroke_alpha: f32,
    /// the innermost q ... Q block the path was drawn in (0 for none)
    pub block: usize,
    /// the innermost marked-content sequence, as an index of `marked`
    pub marked: Option<usize>,
    /// the clip in force when painting, as page-space bounds
    pub clip_bounds: Option<[f32; 4]>,
}

/// one marked-content sequence (BMC or BDC ... EMC).
#[derive(Clone, Debug)]
pub(crate) struct MarkedRecord {
    pub source: usize,
    /// the BMC or BDC operator; its properties are read from it when needed
    pub begin_op: usize,
    pub tag: Vec<u8>,
}

/// one piece of a show operator: a glyph or a TJ position adjustment.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum ShowItem {
    Glyph(usize),
    Adjust(f32),
}

/// a text show operator (Tj, TJ, ' or ").
#[derive(Clone, Debug)]
pub(crate) struct ShowOp {
    pub source: usize,
    pub op: usize,
    /// the state just before the glyphs ("'" and '"' already applied)
    pub before: TextState,
    /// text and line matrices after the operator
    pub text_matrix_after: Matrix,
    pub line_matrix_after: Matrix,
    pub ctm: Matrix,
    pub items: Vec<ShowItem>,
    /// the innermost marked-content sequence, as an index of `marked`
    pub marked: Option<usize>,
}

/// one drawn glyph.
#[derive(Clone, Debug)]
pub(crate) struct GlyphRecord {
    pub show: usize,
    pub text: String,
    pub code: Vec<u8>,
    /// the glyph's width in thousandths of the font size
    pub width: f32,
    pub font: Option<usize>,
    /// the text matrix just before the glyph
    pub text_matrix: Matrix,
    /// how far the glyph moves the text position, in text space
    pub advance: f32,
    pub is_space: bool,
    /// baseline start and end, and the glyph box, in page user space
    pub origin: Point,
    pub end: Point,
    pub corners: [Point; 4],
    pub size: f32,
    pub color: Color,
    pub invisible: bool,
}

/// one image drawn with Do or written inline (BI ... EI).
#[derive(Clone, Debug)]
pub(crate) struct ImageRecord {
    pub source: usize,
    pub op: usize,
    /// the image object, none for an inline image
    pub object: Option<ObjectId>,
    /// maps the unit square onto the page (page user space)
    pub ctm: Matrix,
    pub inline: Option<Dictionary>,
    /// the clip in force when drawing, as page-space bounds
    pub clip_bounds: Option<[f32; 4]>,
    pub fill_alpha: f32,
}

/// everything found on a page.
pub(crate) struct Interpretation {
    pub page_id: ObjectId,
    pub space: PageSpace,
    pub sources: Vec<Source>,
    pub fonts: Vec<PdfFont>,
    pub shows: Vec<ShowOp>,
    pub glyphs: Vec<GlyphRecord>,
    pub images: Vec<ImageRecord>,
    pub paths: Vec<PathRecord>,
    pub marked: Vec<MarkedRecord>,
}

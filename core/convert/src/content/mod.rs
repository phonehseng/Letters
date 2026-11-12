//! reading the vector drawing of a page: the lines, boxes and shapes it
//! paints and where its pictures go.
//!
//! letters-edit reads a page's text; this reads everything else. ruled
//! tables are found from the lines, slides get simple shapes as real
//! shapes, and backgrounds are drawn without what was taken out (see
//! `strip`).

mod colors;
pub(crate) mod strip;

use lopdf::content::{Content, Operation};
use lopdf::{Dictionary, Document, Object, ObjectId};

use crate::geometry::{Matrix, Rect};
use crate::source::{inherited, number, resolve, PageSpace};
use crate::ConvertError;
pub use colors::Rgb;
use colors::{ColorSpaceKind, PaintColor};

/// where an operator is: which of the page's content streams, and its
/// position in that stream. things drawn inside a form have none, since
/// a form can be shared and its operators are not changed here.
pub(crate) type OpRef = (usize, usize);

/// everything a page paints apart from text.
#[derive(Debug, Default)]
pub(crate) struct PageDrawing {
    pub paths: Vec<PaintedPath>,
    pub images: Vec<PlacedImage>,
    /// paint that is neither a path nor a picture placed by `Do` from the
    /// page itself: shadings, inline pictures, unreadable forms
    pub other_paint: usize,
}

/// one painted path, in points on the shown page.
#[derive(Clone, Debug)]
pub(crate) struct PaintedPath {
    pub origin: Option<OpRef>,
    pub subpaths: Vec<Subpath>,
    pub fill: Option<Rgb>,
    pub stroke: Option<Rgb>,
    /// the stroke width as seen on the page
    pub line_width: f32,
    pub has_curves: bool,
    /// drawn with no clipping, no transparency and plain colours, so a
    /// copy of it looks the same anywhere
    pub plain: bool,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct Subpath {
    pub points: Vec<(f32, f32)>,
    pub closed: bool,
}

/// a picture placed with `Do`.
#[derive(Clone, Debug)]
pub(crate) struct PlacedImage {
    pub origin: Option<OpRef>,
    pub rect: Rect,
    /// drawn the usual way up: not turned, slanted or mirrored, so a
    /// copy placed in the same box looks the same
    pub upright: bool,
    pub plain: bool,
}

/// a straight horizontal or vertical stroke, from stroked lines and from
/// boxes thin enough to be lines.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct RuleLine {
    pub horizontal: bool,
    /// y of a horizontal line, x of a vertical one
    pub position: f32,
    pub start: f32,
    pub end: f32,
    pub width: f32,
    pub color: Rgb,
}

/// a filled box, used for table cell shading.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct FilledBox {
    pub rect: Rect,
    pub color: Rgb,
}

/// boxes thinner than this are drawn lines, not shading
const THIN: f32 = 2.5;
/// how deep forms inside forms are followed
const MAX_FORM_DEPTH: usize = 8;

/// reads everything a page paints apart from text.
pub(crate) fn read_page_drawing(
    doc: &Document,
    page_id: ObjectId,
) -> Result<PageDrawing, ConvertError> {
    let page = doc
        .get_dictionary(page_id)
        .map_err(|_| ConvertError::Damaged("a page is missing".into()))?;
    let space = PageSpace::of_page(doc, page_id)?;
    let resources = inherited(doc, page, b"Resources").and_then(|r| r.as_dict().ok());
    let mut reader = Reader {
        doc,
        drawing: PageDrawing::default(),
    };
    let mut state = State::new(space.to_shown);
    for (index, stream) in page_content_streams(doc, page).into_iter().enumerate() {
        let operations = decode_stream(doc, stream);
        // the streams of /Contents are one drawing split in parts, so the
        // graphics state carries on from one into the next
        state = reader.run(&operations, resources, state, Some(index), 0);
    }
    Ok(reader.drawing)
}

/// the content stream objects of a page, in order.
pub(crate) fn page_content_streams(doc: &Document, page: &Dictionary) -> Vec<ObjectId> {
    let parts = match page.get(b"Contents") {
        Ok(Object::Reference(id)) => match doc.get_object(*id) {
            Ok(Object::Array(parts)) => parts.clone(),
            _ => return vec![*id],
        },
        Ok(Object::Array(parts)) => parts.clone(),
        _ => Vec::new(),
    };
    parts
        .iter()
        .filter_map(|part| part.as_reference().ok())
        .collect()
}

/// the operators of a content stream; nothing for a damaged one.
pub(crate) fn decode_stream(doc: &Document, id: ObjectId) -> Vec<Operation> {
    let Ok(stream) = doc.get_object(id).and_then(Object::as_stream) else {
        return Vec::new();
    };
    let bytes = stream
        .decompressed_content()
        .unwrap_or_else(|_| stream.content.clone());
    Content::decode(&bytes)
        .map(|content| content.operations)
        .unwrap_or_default()
}

/// the graphics state that matters here.
#[derive(Clone, Debug)]
struct State {
    ctm: Matrix,
    fill: PaintColor,
    stroke: PaintColor,
    line_width: f32,
    /// a clipping path was set at this level or above
    clipped: bool,
    /// an extended graphics state was set (it may hold transparency or
    /// blend modes, which a copied shape would lose)
    special: bool,
}

impl State {
    fn new(to_shown: Matrix) -> Self {
        Self {
            ctm: to_shown,
            fill: PaintColor::black(),
            stroke: PaintColor::black(),
            line_width: 1.0,
            clipped: false,
            special: false,
        }
    }
}

struct Reader<'a> {
    doc: &'a Document,
    drawing: PageDrawing,
}

/// a path being built, in points on the shown page.
#[derive(Default)]
struct PathInProgress {
    subpaths: Vec<Subpath>,
    has_curves: bool,
    /// a `W` or `W*` came before the painting operator
    clip_pending: bool,
}

impl PathInProgress {
    fn current(&mut self) -> &mut Subpath {
        if self.subpaths.is_empty() {
            self.subpaths.push(Subpath::default());
        }
        self.subpaths.last_mut().expect("just added")
    }
}

impl<'a> Reader<'a> {
    fn run(
        &mut self,
        operations: &[Operation],
        resources: Option<&'a Dictionary>,
        mut state: State,
        stream: Option<usize>,
        depth: usize,
    ) -> State {
        let mut stack: Vec<State> = Vec::new();
        let mut path = PathInProgress::default();
        for (index, operation) in operations.iter().enumerate() {
            let numbers: Vec<f32> = operation.operands.iter().filter_map(number).collect();
            let origin = stream.map(|stream| (stream, index));
            match operation.operator.as_str() {
                "q" => stack.push(state.clone()),
                "Q" => {
                    if let Some(saved) = stack.pop() {
                        state = saved;
                    }
                }
                "cm" => {
                    if let Some(matrix) = Matrix::from_numbers(&numbers) {
                        state.ctm = matrix.then(&state.ctm);
                    }
                }
                "w" => state.line_width = numbers.first().copied().unwrap_or(1.0),
                "gs" => state.special = true,
                "W" | "W*" => path.clip_pending = true,
                "m" => self.move_to(&mut path, &state, &numbers),
                "l" => self.line_to(&mut path, &state, &numbers),
                "c" | "v" | "y" => self.curve_to(&mut path, &state, &numbers),
                "h" => path.current().closed = true,
                "re" => self.rectangle(&mut path, &state, &numbers),
                "S" | "s" | "f" | "F" | "f*" | "B" | "B*" | "b" | "b*" | "n" => {
                    self.paint(&mut path, &mut state, &operation.operator, origin)
                }
                "sh" => self.drawing.other_paint += 1,
                "BI" => self.drawing.other_paint += 1,
                "Do" => self.draw_object(operation, resources, &state, origin, depth),
                operator => colors::apply(
                    operator,
                    &operation.operands,
                    &numbers,
                    &mut state.fill,
                    &mut state.stroke,
                    |name| color_space_kind(self.doc, resources, name),
                ),
            }
        }
        state
    }

    fn move_to(&self, path: &mut PathInProgress, state: &State, numbers: &[f32]) {
        if let [x, y] = numbers {
            path.subpaths.push(Subpath {
                points: vec![state.ctm.apply(*x, *y)],
                closed: false,
            });
        }
    }

    fn line_to(&self, path: &mut PathInProgress, state: &State, numbers: &[f32]) {
        if let [x, y] = numbers {
            path.current().points.push(state.ctm.apply(*x, *y));
        }
    }

    fn curve_to(&self, path: &mut PathInProgress, state: &State, numbers: &[f32]) {
        // only the end point is kept: curves are never turned into simple
        // shapes, so their exact bend doesn't matter here
        if numbers.len() >= 4 {
            let (x, y) = (numbers[numbers.len() - 2], numbers[numbers.len() - 1]);
            path.current().points.push(state.ctm.apply(x, y));
            path.has_curves = true;
        }
    }

    fn rectangle(&self, path: &mut PathInProgress, state: &State, numbers: &[f32]) {
        if let [x, y, w, h] = numbers {
            let corners = [(*x, *y), (x + w, *y), (x + w, y + h), (*x, y + h)];
            path.subpaths.push(Subpath {
                points: corners
                    .iter()
                    .map(|(x, y)| state.ctm.apply(*x, *y))
                    .collect(),
                closed: true,
            });
        }
    }

    fn paint(
        &mut self,
        path: &mut PathInProgress,
        state: &mut State,
        operator: &str,
        origin: Option<OpRef>,
    ) {
        let built = std::mem::take(path);
        if built.clip_pending {
            // the path also clips what comes after it, until the state is
            // restored
            state.clipped = true;
        }
        let fills = matches!(operator, "f" | "F" | "f*" | "B" | "B*" | "b" | "b*");
        let strokes = matches!(operator, "S" | "s" | "B" | "B*" | "b" | "b*");
        if !fills && !strokes {
            return;
        }
        let mut subpaths = built.subpaths;
        if matches!(operator, "s" | "b" | "b*") {
            if let Some(last) = subpaths.last_mut() {
                last.closed = true;
            }
        }
        let fill = fills.then_some(state.fill.rgb).flatten();
        let stroke = strokes.then_some(state.stroke.rgb).flatten();
        let colors_known = (!fills || fill.is_some()) && (!strokes || stroke.is_some());
        self.drawing.paths.push(PaintedPath {
            origin,
            subpaths,
            fill,
            stroke,
            line_width: (state.line_width * state.ctm.average_scale()).max(0.0),
            has_curves: built.has_curves,
            plain: colors_known && !state.clipped && !state.special && !built.clip_pending,
        });
    }

    fn draw_object(
        &mut self,
        operation: &Operation,
        resources: Option<&'a Dictionary>,
        state: &State,
        origin: Option<OpRef>,
        depth: usize,
    ) {
        let Some(Object::Name(name)) = operation.operands.first() else {
            return;
        };
        let Some(object_ref) = resources
            .and_then(|r| r.get(b"XObject").ok())
            .and_then(|x| resolve(self.doc, x).as_dict().ok())
            .and_then(|x| x.get(name).ok())
        else {
            return;
        };
        let Ok(stream) = resolve(self.doc, object_ref).as_stream() else {
            return;
        };
        match stream.dict.get(b"Subtype").and_then(Object::as_name) {
            Ok(b"Image") => self.place_image(state, origin),
            Ok(b"Form") if depth < MAX_FORM_DEPTH => {
                let Ok(id) = object_ref.as_reference() else {
                    self.drawing.other_paint += 1;
                    return;
                };
                self.run_form(id, stream, resources, state, depth)
            }
            _ => self.drawing.other_paint += 1,
        }
    }

    fn place_image(&mut self, state: &State, origin: Option<OpRef>) {
        // a picture fills the unit square of its matrix
        let corners = [(0.0, 0.0), (1.0, 0.0), (0.0, 1.0), (1.0, 1.0)];
        let points: Vec<(f32, f32)> = corners
            .iter()
            .map(|(x, y)| state.ctm.apply(*x, *y))
            .collect();
        // the picture's top row is at the top of the unit square, so on
        // the shown page (y going down) the usual way up has d below zero
        let m = state.ctm;
        let upright = m.a > 0.0 && m.d < 0.0 && m.b.abs() < 1e-4 && m.c.abs() < 1e-4;
        self.drawing.images.push(PlacedImage {
            origin,
            rect: bounds(&points),
            upright,
            plain: !state.clipped && !state.special,
        });
    }

    fn run_form(
        &mut self,
        id: ObjectId,
        stream: &'a lopdf::Stream,
        page_resources: Option<&'a Dictionary>,
        state: &State,
        depth: usize,
    ) {
        let operations = decode_stream(self.doc, id);
        let matrix = stream
            .dict
            .get(b"Matrix")
            .map(|m| crate::source::numbers(resolve(self.doc, m)))
            .ok()
            .and_then(|m| Matrix::from_numbers(&m))
            .unwrap_or(Matrix::IDENTITY);
        let resources = stream
            .dict
            .get(b"Resources")
            .ok()
            .and_then(|r| resolve(self.doc, r).as_dict().ok())
            .or(page_resources);
        let mut inner = state.clone();
        inner.ctm = matrix.then(&state.ctm);
        // a form clips to its bounding box, so nothing in it is unclipped
        // unless the box covers it; treat it as clipped to be safe
        inner.clipped = true;
        self.run(&operations, resources, inner, None, depth + 1);
    }
}

fn color_space_kind(doc: &Document, resources: Option<&Dictionary>, name: &[u8]) -> ColorSpaceKind {
    match name {
        b"DeviceGray" | b"G" => return ColorSpaceKind::Gray,
        b"DeviceRGB" | b"RGB" => return ColorSpaceKind::Rgb,
        b"DeviceCMYK" | b"CMYK" => return ColorSpaceKind::Cmyk,
        _ => {}
    }
    let Some(space) = resources
        .and_then(|r| r.get(b"ColorSpace").ok())
        .and_then(|c| resolve(doc, c).as_dict().ok())
        .and_then(|c| c.get(name).ok())
        .map(|space| resolve(doc, space))
    else {
        return ColorSpaceKind::Other;
    };
    colors::kind_of_space(doc, space)
}

/// the smallest upright box around some points.
pub(crate) fn bounds(points: &[(f32, f32)]) -> Rect {
    let min_x = points.iter().map(|p| p.0).fold(f32::INFINITY, f32::min);
    let max_x = points.iter().map(|p| p.0).fold(f32::NEG_INFINITY, f32::max);
    let min_y = points.iter().map(|p| p.1).fold(f32::INFINITY, f32::min);
    let max_y = points.iter().map(|p| p.1).fold(f32::NEG_INFINITY, f32::max);
    if !min_x.is_finite() {
        return Rect::default();
    }
    Rect::from_corners(min_x, min_y, max_x, max_y)
}

impl PaintedPath {
    /// the upright box this path is, if it is exactly one (four corners
    /// lined up with the page).
    pub fn as_box(&self) -> Option<Rect> {
        if self.has_curves || self.subpaths.len() != 1 {
            return None;
        }
        let points = &self.subpaths[0].points;
        let mut corners: Vec<(f32, f32)> = points.clone();
        if corners.len() == 5 && near(corners[0], corners[4]) {
            corners.pop();
        }
        if corners.len() != 4 {
            return None;
        }
        let rect = bounds(&corners);
        let on_corner = |p: &(f32, f32)| {
            (close(p.0, rect.x) || close(p.0, rect.right()))
                && (close(p.1, rect.y) || close(p.1, rect.bottom()))
        };
        corners.iter().all(on_corner).then_some(rect)
    }

    /// the horizontal and vertical strokes this path draws.
    pub fn rule_lines(&self) -> Vec<RuleLine> {
        let mut lines = Vec::new();
        if let (Some(color), Some(rect)) = (self.fill, self.as_box()) {
            // a thin filled box is how many programs draw a rule
            if rect.height <= THIN && rect.width > rect.height * 3.0 {
                lines.push(RuleLine {
                    horizontal: true,
                    position: rect.center_y(),
                    start: rect.x,
                    end: rect.right(),
                    width: rect.height.max(0.1),
                    color,
                });
            } else if rect.width <= THIN && rect.height > rect.width * 3.0 {
                lines.push(RuleLine {
                    horizontal: false,
                    position: rect.center_x(),
                    start: rect.y,
                    end: rect.bottom(),
                    width: rect.width.max(0.1),
                    color,
                });
            }
        }
        let Some(color) = self.stroke else {
            return lines;
        };
        if self.has_curves {
            return lines;
        }
        for subpath in &self.subpaths {
            let mut points = subpath.points.clone();
            if subpath.closed && points.len() > 2 {
                points.push(points[0]);
            }
            for pair in points.windows(2) {
                if let Some(line) = straight_line(pair[0], pair[1], self.line_width, color) {
                    lines.push(line);
                }
            }
        }
        lines
    }

    /// the shading box this path fills, if it is a box too big to be a line.
    pub fn filled_box(&self) -> Option<FilledBox> {
        let color = self.fill?;
        let rect = self.as_box()?;
        (rect.width > THIN && rect.height > THIN).then_some(FilledBox { rect, color })
    }
}

fn straight_line(a: (f32, f32), b: (f32, f32), width: f32, color: Rgb) -> Option<RuleLine> {
    let tolerance = 0.5;
    if (a.1 - b.1).abs() < tolerance && (a.0 - b.0).abs() > tolerance {
        Some(RuleLine {
            horizontal: true,
            position: (a.1 + b.1) / 2.0,
            start: a.0.min(b.0),
            end: a.0.max(b.0),
            width,
            color,
        })
    } else if (a.0 - b.0).abs() < tolerance && (a.1 - b.1).abs() > tolerance {
        Some(RuleLine {
            horizontal: false,
            position: (a.0 + b.0) / 2.0,
            start: a.1.min(b.1),
            end: a.1.max(b.1),
            width,
            color,
        })
    } else {
        None
    }
}

fn close(a: f32, b: f32) -> bool {
    (a - b).abs() < 0.05
}

fn near(a: (f32, f32), b: (f32, f32)) -> bool {
    close(a.0, b.0) && close(a.1, b.1)
}

impl PageDrawing {
    pub fn rule_lines(&self) -> Vec<RuleLine> {
        self.paths
            .iter()
            .flat_map(PaintedPath::rule_lines)
            .collect()
    }

    pub fn filled_boxes(&self) -> Vec<FilledBox> {
        self.paths
            .iter()
            .filter_map(PaintedPath::filled_box)
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn drawing_of(content: &str) -> PageDrawing {
        let (doc, page_id) = crate::test_pdf::one_page_document(content);
        read_page_drawing(&doc, page_id).unwrap()
    }

    #[test]
    fn pictures_are_placed_by_their_matrix() {
        let drawing = drawing_of("q 100 0 0 50 72 600 cm /Im1 Do Q");
        assert_eq!(drawing.images.len(), 1);
        assert_eq!(drawing.images[0].rect, Rect::new(72.0, 150.0, 100.0, 50.0));
        assert!(drawing.images[0].upright);
        assert_eq!(drawing.images[0].origin, Some((0, 2)));
    }

    #[test]
    fn stroked_boxes_become_four_rules() {
        let drawing = drawing_of("0 0 1 RG 2 w 100 700 200 50 re S");
        let lines = drawing.rule_lines();
        assert_eq!(lines.len(), 4);
        // the box's top edge, at y 700 in pdf space, is 100 from the top
        let top = lines
            .iter()
            .find(|l| l.horizontal && (l.position - 50.0).abs() < 0.01)
            .unwrap();
        assert_eq!((top.start, top.end, top.width), (100.0, 300.0, 2.0));
        assert_eq!(top.color, Rgb(0, 0, 255));
        assert!(drawing.paths[0].plain);
    }

    #[test]
    fn thin_filled_boxes_are_rules_and_big_ones_shading() {
        let drawing = drawing_of("0.5 g 72 700 400 1 re f 1 0 0 rg 72 500 100 100 re f");
        let lines = drawing.rule_lines();
        assert_eq!(lines.len(), 1);
        assert!(lines[0].horizontal);
        assert_eq!(lines[0].color, Rgb(128, 128, 128));
        let boxes = drawing.filled_boxes();
        assert_eq!(boxes.len(), 1);
        assert_eq!(boxes[0].rect, Rect::new(72.0, 200.0, 100.0, 100.0));
        assert_eq!(boxes[0].color, Rgb(255, 0, 0));
    }

    #[test]
    fn transformed_lines_land_on_the_page() {
        let drawing = drawing_of("q 2 0 0 2 10 10 cm 0 0 m 50 0 l S Q");
        let line = drawing.rule_lines()[0];
        assert_eq!((line.start, line.end), (10.0, 110.0));
        assert_eq!(line.position, 790.0);
        assert_eq!(line.width, 2.0);
    }

    #[test]
    fn clipped_and_curved_paths_are_not_plain_boxes() {
        let drawing =
            drawing_of("q 0 0 100 100 re W n 10 10 50 50 re f Q 0 0 m 10 10 20 20 30 0 c f");
        assert!(!drawing.paths[0].plain);
        assert!(drawing.paths[1].has_curves);
        assert_eq!(drawing.paths[1].as_box(), None);
    }

    #[test]
    fn operators_are_located() {
        let drawing = drawing_of("q 0 0 m 10 0 l S Q");
        assert_eq!(drawing.paths[0].origin, Some((0, 3)));
    }
}

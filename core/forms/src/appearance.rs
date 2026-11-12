//! drawing what a field looks like (its "appearance stream").
//!
//! a widget's look is a small drawing stored in the file under /AP. a pdf
//! can instead ask the reader to draw it (/NeedAppearances), but readers
//! do that differently or not at all, so letters always draws it: the
//! answer, in the field's own font and size, wrapped, aligned and cut to
//! the box, plus the box's background and border.

use lopdf::content::Content;
use lopdf::{Dictionary, Document, Object, ObjectId, Stream};

use crate::error::{FormError, FormResult};
use crate::fonts::{metrics_of, FontMetrics, StandardFont};
use crate::geometry::{number, Matrix, PdfRect};
use crate::objects::{
    acroform_id, get, get_array, get_dict, get_integer, get_name, get_rect, name, number_of,
    rect_object, resolve,
};
use crate::read::{kind_of, Alignment, FieldKind};
use crate::text::{
    encode_win_ansi, encode_win_ansi_lines, line_breaks_to_spaces, literal_string,
    normalize_line_breaks,
};
use crate::tree::{flags, on_state, options_of, FieldNode};

/// the resource name letters gives its own helvetica in the form's fonts.
pub const DEFAULT_FONT_NAME: &str = "Helv";

/// the font size used when a field asks for automatic size and the text
/// spans several lines.
const AUTO_SIZE_MULTILINE_START: f32 = 12.0;
const AUTO_SIZE_MIN: f32 = 4.0;
const AUTO_SIZE_MAX: f32 = 24.0;
/// distance between lines as a share of the font size
const LINE_SPACING: f32 = 1.15;

/// the parts of a /DA string ("/Helv 12 Tf 0 g") that matter here.
#[derive(Clone, Debug, PartialEq)]
pub struct DefaultAppearance {
    pub font: Option<String>,
    /// 0 means "fit the text to the field"
    pub size: f32,
    /// the colour operator that sets the fill colour, like "0 g"
    pub color: String,
}

impl DefaultAppearance {
    pub fn parse(da: &[u8]) -> Self {
        let mut parsed = Self {
            font: None,
            size: 0.0,
            color: "0 g".to_owned(),
        };
        let Ok(content) = Content::decode(da) else {
            return parsed;
        };
        for operation in content.operations {
            let numbers: Vec<String> = operation
                .operands
                .iter()
                .filter_map(number_of)
                .map(number)
                .collect();
            match operation.operator.as_str() {
                "Tf" => {
                    parsed.font = operation
                        .operands
                        .first()
                        .and_then(|operand| operand.as_name().ok())
                        .map(|font| String::from_utf8_lossy(font).into_owned());
                    parsed.size = operation
                        .operands
                        .get(1)
                        .and_then(number_of)
                        .unwrap_or(0.0)
                        .max(0.0);
                }
                "g" | "rg" | "k" if numbers.len() == operation.operands.len() => {
                    parsed.color = format!("{} {}", numbers.join(" "), operation.operator);
                }
                _ => {}
            }
        }
        parsed
    }

    /// the same colour as a stroke colour, for check marks drawn as lines.
    pub fn stroke_color(&self) -> String {
        stroke_version(&self.color)
    }
}

fn stroke_version(fill: &str) -> String {
    if let Some(start) = fill.strip_suffix(" rg") {
        format!("{start} RG")
    } else if let Some(start) = fill.strip_suffix(" k") {
        format!("{start} K")
    } else if let Some(start) = fill.strip_suffix(" g") {
        format!("{start} G")
    } else {
        "0 G".to_owned()
    }
}

/// a font ready to be used in an appearance.
#[derive(Clone, Debug)]
pub struct AppearanceFont {
    /// the name the drawing uses for it in its own resources
    pub resource_name: String,
    pub object: Object,
    pub metrics: FontMetrics,
}

/// finds the font a field's /DA asks for in the form's fonts (/DR). when
/// it is missing, or can't draw the text (fonts with two-byte codes, or a
/// font without winansi for non-ascii text), helvetica is used instead.
pub fn resolve_font(
    doc: &mut Document,
    wanted: Option<&str>,
    needs_win_ansi: bool,
) -> FormResult<AppearanceFont> {
    if let Some(wanted) = wanted {
        if let Some(font) = form_font(doc, wanted) {
            if font_can_draw(doc, &font, needs_win_ansi) {
                return Ok(AppearanceFont {
                    resource_name: wanted.to_owned(),
                    metrics: metrics_of(doc, &font),
                    object: font,
                });
            }
        }
    }
    let helvetica = ensure_default_font(doc)?;
    Ok(AppearanceFont {
        resource_name: DEFAULT_FONT_NAME.to_owned(),
        object: Object::Reference(helvetica),
        metrics: FontMetrics::standard(StandardFont::Helvetica),
    })
}

/// the font object called `wanted` in /AcroForm /DR /Font.
fn form_font(doc: &Document, wanted: &str) -> Option<Object> {
    let form = crate::objects::acroform(doc)?;
    let resources = get_dict(doc, form, b"DR")?;
    let fonts = get_dict(doc, resources, b"Font")?;
    fonts.get(wanted.as_bytes()).ok().cloned()
}

fn font_can_draw(doc: &Document, font: &Object, needs_win_ansi: bool) -> bool {
    let Ok(dict) = resolve(doc, font).as_dict() else {
        return false;
    };
    let simple = matches!(
        get_name(doc, dict, b"Subtype").as_deref(),
        Some(b"Type1") | Some(b"TrueType") | Some(b"MMType1")
    );
    let win_ansi = get_name(doc, dict, b"Encoding").as_deref() == Some(b"WinAnsiEncoding");
    simple && (win_ansi || !needs_win_ansi)
}

/// makes sure the form has letters' helvetica in its fonts and returns it.
/// a form whose /Helv is something else keeps it; ours is then added
/// under another name only inside each appearance.
pub fn ensure_default_font(doc: &mut Document) -> FormResult<ObjectId> {
    if let Some(Object::Reference(id)) = form_font(doc, DEFAULT_FONT_NAME) {
        if font_can_draw(doc, &Object::Reference(id), true) {
            return Ok(id);
        }
    }
    let font = doc.add_object(standard_font_dict(StandardFont::Helvetica));
    if let Some(form_id) = acroform_id(doc)? {
        let has_helv = form_font(doc, DEFAULT_FONT_NAME).is_some();
        if !has_helv {
            add_form_font(doc, form_id, DEFAULT_FONT_NAME, font)?;
        }
    }
    Ok(font)
}

/// a dictionary for one of the standard fonts in winansi encoding.
pub fn standard_font_dict(font: StandardFont) -> Dictionary {
    let mut dict = Dictionary::new();
    dict.set("Type", name("Font"));
    dict.set("Subtype", name("Type1"));
    dict.set("BaseFont", name(font.base_font_name()));
    dict.set("Encoding", name("WinAnsiEncoding"));
    dict
}

/// adds a font to /AcroForm /DR /Font, making the dictionaries on the way.
pub fn add_form_font(
    doc: &mut Document,
    form_id: ObjectId,
    font_name: &str,
    font: ObjectId,
) -> FormResult<()> {
    let form = doc.get_dictionary(form_id)?.clone();
    let mut resources = match form.get(b"DR").ok().map(|value| resolve(doc, value)) {
        Some(Object::Dictionary(dict)) => dict.clone(),
        _ => Dictionary::new(),
    };
    let mut fonts = match resources.get(b"Font").ok().map(|value| resolve(doc, value)) {
        Some(Object::Dictionary(dict)) => dict.clone(),
        _ => Dictionary::new(),
    };
    fonts.set(font_name.as_bytes().to_vec(), Object::Reference(font));
    resources.set("Font", Object::Dictionary(fonts));
    doc.get_dictionary_mut(form_id)?
        .set("DR", Object::Dictionary(resources));
    Ok(())
}

/// the box a widget draws into, in its own (possibly turned) space.
#[derive(Clone, Debug, PartialEq)]
pub struct WidgetBox {
    pub width: f32,
    pub height: f32,
    /// turns the drawing to match /MK /R, so text reads upright on turned pages
    pub matrix: Matrix,
    pub border_width: f32,
    /// stroke colour operator, when the widget has a border colour
    pub border_color: Option<String>,
    /// fill colour operator, when the widget has a background
    pub background: Option<String>,
    /// /BS /S: solid, dashed, beveled, inset or underline
    pub border_style: u8,
}

impl WidgetBox {
    pub fn read(doc: &Document, widget: ObjectId) -> Option<Self> {
        let dict = doc.get_dictionary(widget).ok()?;
        let rect = get_rect(doc, dict, b"Rect")?;
        let characteristics = get_dict(doc, dict, b"MK");
        let rotation = characteristics
            .and_then(|mk| get_integer(doc, mk, b"R"))
            .unwrap_or(0)
            .rem_euclid(360) as i32;
        let (width, height) = if rotation % 180 == 0 {
            (rect.width(), rect.height())
        } else {
            (rect.height(), rect.width())
        };
        let color_of = |key: &[u8], operators: [&str; 3]| {
            let values = characteristics.and_then(|mk| get_array(doc, mk, key))?;
            color_operator(doc, values, operators)
        };
        let border_style = get_dict(doc, dict, b"BS");
        let border_width = border_style
            .and_then(|bs| get(doc, bs, b"W").and_then(number_of))
            .or_else(|| {
                let border = get_array(doc, dict, b"Border")?;
                border
                    .get(2)
                    .and_then(|value| number_of(resolve(doc, value)))
            })
            .unwrap_or(1.0)
            .max(0.0);
        let style = border_style
            .and_then(|bs| get_name(doc, bs, b"S"))
            .and_then(|style| style.first().copied())
            .unwrap_or(b'S');
        let border_color = color_of(b"BC", ["G", "RG", "K"]);
        Some(Self {
            width,
            height,
            matrix: upright_matrix(rotation, width, height),
            border_width: if border_color.is_some() {
                border_width
            } else {
                0.0
            },
            border_color,
            background: color_of(b"BG", ["g", "rg", "k"]),
            border_style: style,
        })
    }

    /// the bounding box of the drawing, [0 0 width height].
    pub fn bbox(&self) -> PdfRect {
        PdfRect::from_corners(0.0, 0.0, self.width, self.height)
    }
}

/// the /Matrix that turns a drawing of `width` by `height` counterclockwise
/// by `rotation` degrees and moves it back so it starts at 0, 0.
pub(crate) fn upright_matrix(rotation: i32, width: f32, height: f32) -> Matrix {
    let turn = Matrix::quarter_turns(rotation);
    let moved = turn.apply_rect(&PdfRect::from_corners(0.0, 0.0, width, height));
    turn.then(&Matrix::translate(-moved.left, -moved.bottom))
}

/// a colour from /MK as an operator. no components means "no colour".
fn color_operator(doc: &Document, values: &[Object], operators: [&str; 3]) -> Option<String> {
    let numbers: Vec<String> = values
        .iter()
        .filter_map(|value| number_of(resolve(doc, value)))
        .map(number)
        .collect();
    let operator = match numbers.len() {
        1 => operators[0],
        3 => operators[1],
        4 => operators[2],
        _ => return None,
    };
    Some(format!("{} {operator}", numbers.join(" ")))
}

/// a drawing to be stored as an appearance stream.
#[derive(Clone, Debug)]
pub struct Appearance {
    pub content: String,
    pub fonts: Vec<(String, Object)>,
}

/// the background and border every kind of field starts with.
fn frame_content(widget: &WidgetBox, round: bool) -> String {
    let mut out = String::new();
    let (w, h) = (widget.width, widget.height);
    if let Some(background) = &widget.background {
        out.push_str(&format!("{background}\n"));
        if round {
            out.push_str(&circle_path(w / 2.0, h / 2.0, w.min(h) / 2.0));
        } else {
            out.push_str(&format!("0 0 {} {} re\n", number(w), number(h)));
        }
        out.push_str("f\n");
    }
    if let Some(border) = &widget.border_color {
        let line = widget.border_width;
        if line > 0.0 {
            out.push_str(&format!("{border}\n{} w\n", number(line)));
            if widget.border_style == b'D' {
                out.push_str("[3] 0 d\n");
            }
            let half = line / 2.0;
            if round {
                out.push_str(&circle_path(w / 2.0, h / 2.0, w.min(h) / 2.0 - half));
            } else if widget.border_style == b'U' {
                out.push_str(&format!("0 {0} m {1} {0} l\n", number(half), number(w)));
            } else {
                out.push_str(&format!(
                    "{0} {0} {1} {2} re\n",
                    number(half),
                    number(w - line),
                    number(h - line)
                ));
            }
            out.push_str("S\n");
        }
    }
    out
}

/// a circle drawn with four curves, the usual way pdf draws circles.
fn circle_path(cx: f32, cy: f32, r: f32) -> String {
    // how far the control points sit from the ends so curves look round
    let k = 0.5523 * r;
    let n = number;
    format!(
        "{} {} m {} {} {} {} {} {} c {} {} {} {} {} {} c {} {} {} {} {} {} c {} {} {} {} {} {} c h\n",
        n(cx + r), n(cy),
        n(cx + r), n(cy + k), n(cx + k), n(cy + r), n(cx), n(cy + r),
        n(cx - k), n(cy + r), n(cx - r), n(cy + k), n(cx - r), n(cy),
        n(cx - r), n(cy - k), n(cx - k), n(cy - r), n(cx), n(cy - r),
        n(cx + k), n(cy - r), n(cx + r), n(cy - k), n(cx + r), n(cy),
    )
}

/// how a text answer should be laid out.
#[derive(Clone, Debug, PartialEq)]
pub struct TextLayout {
    pub text: String,
    pub multiline: bool,
    /// the number of cells of a comb field
    pub comb_cells: Option<u32>,
    pub alignment: Alignment,
}

/// draws a text answer.
pub fn text_appearance(
    widget: &WidgetBox,
    da: &DefaultAppearance,
    font: &AppearanceFont,
    layout: &TextLayout,
) -> FormResult<Appearance> {
    // multiline fields keep the breaks people typed; one line fields (and
    // combs) show them as spaces, since they have nowhere to break
    let bytes = if layout.multiline && layout.comb_cells.is_none() {
        encode_win_ansi_lines(&normalize_line_breaks(&layout.text))
    } else {
        encode_win_ansi(&line_breaks_to_spaces(&layout.text))
    }
    .map_err(FormError::UnsupportedCharacters)?;
    let mut content = frame_content(widget, false);
    content.push_str("/Tx BMC\nq\n");
    let inset = widget.border_width.max(0.0) + 1.0;
    content.push_str(&format!(
        "{} {} {} {} re W n\n",
        number(inset),
        number(inset),
        number((widget.width - 2.0 * inset).max(0.0)),
        number((widget.height - 2.0 * inset).max(0.0))
    ));
    if !bytes.is_empty() {
        let text = if let Some(cells) = layout.comb_cells.filter(|cells| *cells > 0) {
            comb_text(widget, da, font, &bytes, cells)
        } else if layout.multiline {
            multiline_text(widget, da, font, &bytes, layout.alignment)
        } else {
            single_line_text(widget, da, font, &bytes, layout.alignment)
        };
        content.push_str(&text);
    }
    content.push_str("Q\nEMC\n");
    Ok(Appearance {
        content,
        fonts: vec![(font.resource_name.clone(), font.object.clone())],
    })
}

/// where text starts horizontally inside a box for an alignment.
fn aligned_x(widget: &WidgetBox, text_width: f32, alignment: Alignment) -> f32 {
    let padding = widget.border_width + 2.0;
    match alignment {
        Alignment::Left => padding,
        Alignment::Center => (widget.width - text_width) / 2.0,
        Alignment::Right => widget.width - padding - text_width,
    }
}

/// the baseline that centres one line of text of `size` in a box `height` tall.
fn centered_baseline(font: &AppearanceFont, size: f32, height: f32) -> f32 {
    let (ascent, descent) = font.metrics.standard.ascent_descent();
    let text_height = (ascent - descent) * size / 1000.0;
    (height - text_height) / 2.0 - descent * size / 1000.0
}

fn text_start(font: &AppearanceFont, da: &DefaultAppearance, size: f32) -> String {
    format!(
        "BT\n/{} {} Tf\n{}\n",
        font.resource_name,
        number(size),
        da.color
    )
}

fn single_line_text(
    widget: &WidgetBox,
    da: &DefaultAppearance,
    font: &AppearanceFont,
    bytes: &[u8],
    alignment: Alignment,
) -> String {
    let available_width = widget.width - 2.0 * (widget.border_width + 2.0);
    let size = if da.size > 0.0 {
        da.size
    } else {
        let by_height = (widget.height - 2.0 * (widget.border_width + 1.0)) / LINE_SPACING;
        let unit_width = font.metrics.text_width(bytes, 1.0);
        let by_width = if unit_width > 0.0 {
            available_width / unit_width
        } else {
            by_height
        };
        by_height.min(by_width).clamp(AUTO_SIZE_MIN, AUTO_SIZE_MAX)
    };
    let width = font.metrics.text_width(bytes, size);
    let x = aligned_x(widget, width, alignment);
    let y = centered_baseline(font, size, widget.height);
    format!(
        "{}{} {} Td\n{} Tj\nET\n",
        text_start(font, da, size),
        number(x),
        number(y),
        literal_string(bytes)
    )
}

/// a comb field puts each character in its own equal cell.
fn comb_text(
    widget: &WidgetBox,
    da: &DefaultAppearance,
    font: &AppearanceFont,
    bytes: &[u8],
    cells: u32,
) -> String {
    let cell_width = widget.width / cells as f32;
    let size = if da.size > 0.0 {
        da.size
    } else {
        ((widget.height - 2.0 * (widget.border_width + 1.0)) / LINE_SPACING)
            .min(cell_width * 1.4)
            .clamp(AUTO_SIZE_MIN, AUTO_SIZE_MAX)
    };
    let y = centered_baseline(font, size, widget.height);
    let mut out = text_start(font, da, size);
    let mut previous_x = 0.0;
    for (index, byte) in bytes.iter().take(cells as usize).enumerate() {
        let character_width = font.metrics.text_width(&[*byte], size);
        let x = index as f32 * cell_width + (cell_width - character_width) / 2.0;
        let (dx, dy) = if index == 0 {
            (x, y)
        } else {
            (x - previous_x, 0.0)
        };
        out.push_str(&format!(
            "{} {} Td {} Tj\n",
            number(dx),
            number(dy),
            literal_string(&[*byte])
        ));
        previous_x = x;
    }
    out.push_str("ET\n");
    out
}

fn multiline_text(
    widget: &WidgetBox,
    da: &DefaultAppearance,
    font: &AppearanceFont,
    bytes: &[u8],
    alignment: Alignment,
) -> String {
    let padding = widget.border_width + 2.0;
    let available_width = (widget.width - 2.0 * padding).max(1.0);
    let available_height = widget.height - 2.0 * padding;
    let size = if da.size > 0.0 {
        da.size
    } else {
        // shrink until the wrapped text fits the box, but never below the minimum
        let mut size = AUTO_SIZE_MULTILINE_START;
        while size > AUTO_SIZE_MIN {
            let lines = wrap_lines(bytes, &font.metrics, size, available_width);
            if lines.len() as f32 * size * LINE_SPACING <= available_height {
                break;
            }
            size -= 0.5;
        }
        size
    };
    let lines = wrap_lines(bytes, &font.metrics, size, available_width);
    let (ascent, _) = font.metrics.standard.ascent_descent();
    let leading = size * LINE_SPACING;
    let mut y = widget.height - padding - ascent * size / 1000.0;
    let mut out = text_start(font, da, size);
    let mut previous = (0.0, 0.0);
    for line in lines {
        let x = aligned_x(widget, font.metrics.text_width(&line, size), alignment);
        out.push_str(&format!(
            "{} {} Td {} Tj\n",
            number(x - previous.0),
            number(y - previous.1),
            literal_string(&line)
        ));
        previous = (x, y);
        y -= leading;
    }
    out.push_str("ET\n");
    out
}

/// breaks text into lines that fit `width`: at line breaks the person typed,
/// then between words, and inside a word only when the word alone is too long.
pub fn wrap_lines(bytes: &[u8], metrics: &FontMetrics, size: f32, width: f32) -> Vec<Vec<u8>> {
    let text: Vec<u8> = bytes
        .iter()
        .map(|&byte| if byte == b'\r' { b'\n' } else { byte })
        .collect();
    let mut lines = Vec::new();
    for paragraph in text.split(|&byte| byte == b'\n') {
        let mut line: Vec<u8> = Vec::new();
        for word in paragraph.split(|&byte| byte == b' ') {
            let candidate = if line.is_empty() {
                word.to_vec()
            } else {
                [line.as_slice(), b" ", word].concat()
            };
            if metrics.text_width(&candidate, size) <= width {
                line = candidate;
                continue;
            }
            if !line.is_empty() {
                lines.push(std::mem::take(&mut line));
            }
            // a word wider than the line is cut into pieces that fit
            for &byte in word {
                let mut longer = line.clone();
                longer.push(byte);
                if metrics.text_width(&longer, size) > width && !line.is_empty() {
                    lines.push(std::mem::replace(&mut line, vec![byte]));
                } else {
                    line = longer;
                }
            }
        }
        lines.push(line);
    }
    lines
}

/// the check styles of /MK /CA, named by their zapfdingbats letter.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CheckStyle {
    Check,
    Circle,
    Cross,
    Diamond,
    Square,
    Star,
}

impl CheckStyle {
    pub fn from_caption(caption: Option<&str>, radio: bool) -> Self {
        match caption.and_then(|text| text.chars().next()) {
            Some('l') => CheckStyle::Circle,
            Some('8') => CheckStyle::Cross,
            Some('u') => CheckStyle::Diamond,
            Some('n') => CheckStyle::Square,
            Some('H') => CheckStyle::Star,
            Some('4') => CheckStyle::Check,
            _ if radio => CheckStyle::Circle,
            _ => CheckStyle::Check,
        }
    }
}

/// draws a checkbox or radio button. the mark is drawn with lines and
/// curves rather than a font, so it looks the same in every reader.
pub fn check_appearance(
    widget: &WidgetBox,
    da: &DefaultAppearance,
    style: CheckStyle,
    round: bool,
    on: bool,
) -> Appearance {
    let mut content = frame_content(widget, round);
    if on {
        content.push_str("q\n");
        content.push_str(&check_mark(widget, da, style));
        content.push_str("Q\n");
    }
    Appearance {
        content,
        fonts: Vec::new(),
    }
}

fn check_mark(widget: &WidgetBox, da: &DefaultAppearance, style: CheckStyle) -> String {
    let side = widget.width.min(widget.height);
    let (cx, cy) = (widget.width / 2.0, widget.height / 2.0);
    let n = number;
    // a point of the mark, given in a unit square centred on the box
    let at = |u: f32, v: f32| format!("{} {}", n(cx + (u - 0.5) * side), n(cy + (v - 0.5) * side));
    match style {
        CheckStyle::Check => format!(
            "{} {} w 1 J 1 j\n{} m {} l {} l S\n",
            da.stroke_color(),
            n(side * 0.12),
            at(0.2, 0.52),
            at(0.42, 0.28),
            at(0.8, 0.76)
        ),
        CheckStyle::Cross => format!(
            "{} {} w 1 J\n{} m {} l {} m {} l S\n",
            da.stroke_color(),
            n(side * 0.1),
            at(0.25, 0.25),
            at(0.75, 0.75),
            at(0.25, 0.75),
            at(0.75, 0.25)
        ),
        CheckStyle::Circle => format!("{}\n{}f\n", da.color, circle_path(cx, cy, side * 0.25)),
        CheckStyle::Square => format!(
            "{}\n{} {} {} {} re f\n",
            da.color,
            n(cx - side * 0.25),
            n(cy - side * 0.25),
            n(side * 0.5),
            n(side * 0.5)
        ),
        CheckStyle::Diamond => format!(
            "{}\n{} m {} l {} l {} l h f\n",
            da.color,
            at(0.5, 0.2),
            at(0.8, 0.5),
            at(0.5, 0.8),
            at(0.2, 0.5)
        ),
        CheckStyle::Star => {
            let points: Vec<String> = (0..10)
                .map(|index| {
                    let radius = if index % 2 == 0 { 0.32 } else { 0.13 };
                    let angle =
                        std::f32::consts::FRAC_PI_2 + index as f32 * std::f32::consts::PI / 5.0;
                    at(0.5 + radius * angle.cos(), 0.5 + radius * angle.sin())
                })
                .collect();
            format!(
                "{}\n{} m {} l h f\n",
                da.color,
                points[0],
                points[1..].join(" l ")
            )
        }
    }
}

/// draws a list box: one line per choice, chosen ones highlighted.
pub fn list_appearance(
    widget: &WidgetBox,
    da: &DefaultAppearance,
    font: &AppearanceFont,
    labels: &[String],
    chosen: &[bool],
    top_index: usize,
) -> FormResult<Appearance> {
    let size = if da.size > 0.0 {
        da.size
    } else {
        AUTO_SIZE_MULTILINE_START
    };
    let leading = size * LINE_SPACING;
    let padding = widget.border_width + 2.0;
    let mut content = frame_content(widget, false);
    content.push_str("/Tx BMC\nq\n");
    let inset = widget.border_width + 1.0;
    content.push_str(&format!(
        "{} {} {} {} re W n\n",
        number(inset),
        number(inset),
        number((widget.width - 2.0 * inset).max(0.0)),
        number((widget.height - 2.0 * inset).max(0.0))
    ));
    let (ascent, _) = font.metrics.standard.ascent_descent();
    let mut top = widget.height - inset;
    for (index, label) in labels.iter().enumerate().skip(top_index) {
        if top < 0.0 {
            break;
        }
        let bytes = encode_win_ansi(label).map_err(FormError::UnsupportedCharacters)?;
        if chosen.get(index).copied().unwrap_or(false) {
            // the highlight colour acrobat uses for chosen list items
            content.push_str(&format!(
                "0.6 0.75 0.86 rg\n{} {} {} {} re f\n",
                number(inset),
                number(top - leading),
                number(widget.width - 2.0 * inset),
                number(leading)
            ));
        }
        let baseline = top - (leading - size) / 2.0 - ascent * size / 1000.0;
        content.push_str(&format!(
            "{}{} {} Td {} Tj\nET\n",
            text_start(font, da, size),
            number(padding),
            number(baseline),
            literal_string(&bytes)
        ));
        top -= leading;
    }
    content.push_str("Q\nEMC\n");
    Ok(Appearance {
        content,
        fonts: vec![(font.resource_name.clone(), font.object.clone())],
    })
}

/// turns a drawing into a form xobject stream sized for the widget.
pub fn appearance_stream(widget: &WidgetBox, appearance: &Appearance) -> Stream {
    let mut dict = Dictionary::new();
    dict.set("Type", name("XObject"));
    dict.set("Subtype", name("Form"));
    dict.set("BBox", rect_object(&widget.bbox()));
    if widget.matrix != Matrix::IDENTITY {
        let m = widget.matrix;
        dict.set(
            "Matrix",
            Object::Array(
                [m.a, m.b, m.c, m.d, m.e, m.f]
                    .iter()
                    .map(|v| crate::objects::real(*v))
                    .collect(),
            ),
        );
    }
    let mut resources = Dictionary::new();
    if !appearance.fonts.is_empty() {
        let mut fonts = Dictionary::new();
        for (font_name, object) in &appearance.fonts {
            fonts.set(font_name.as_bytes().to_vec(), object.clone());
        }
        resources.set("Font", Object::Dictionary(fonts));
    }
    dict.set("Resources", Object::Dictionary(resources));
    Stream::new(dict, appearance.content.as_bytes().to_vec())
}

/// stores a single normal appearance for a widget (text and choice fields).
pub fn store_appearance(
    doc: &mut Document,
    widget_id: ObjectId,
    widget: &WidgetBox,
    appearance: &Appearance,
) -> FormResult<()> {
    let stream = doc.add_object(appearance_stream(widget, appearance));
    let mut appearances = Dictionary::new();
    appearances.set("N", Object::Reference(stream));
    doc.get_dictionary_mut(widget_id)?
        .set("AP", Object::Dictionary(appearances));
    Ok(())
}

/// stores the on and off drawings of a checkbox or radio widget.
pub fn store_state_appearances(
    doc: &mut Document,
    widget_id: ObjectId,
    widget: &WidgetBox,
    state: &[u8],
    on: &Appearance,
    off: &Appearance,
) -> FormResult<()> {
    let on_stream = doc.add_object(appearance_stream(widget, on));
    let off_stream = doc.add_object(appearance_stream(widget, off));
    let mut states = Dictionary::new();
    states.set(state.to_vec(), Object::Reference(on_stream));
    states.set("Off", Object::Reference(off_stream));
    let mut appearances = Dictionary::new();
    appearances.set("N", Object::Dictionary(states));
    doc.get_dictionary_mut(widget_id)?
        .set("AP", Object::Dictionary(appearances));
    Ok(())
}

/// draws every widget of a field again from its current value.
///
/// checkboxes and radio buttons keep the drawings their author made unless
/// `redraw_buttons` is set (a new or resized field) or they have none.
pub fn refresh_field(
    doc: &mut Document,
    field: &FieldNode,
    redraw_buttons: bool,
) -> FormResult<()> {
    let Some(kind) = kind_of(doc, field) else {
        return Ok(());
    };
    match kind {
        FieldKind::Text | FieldKind::MultilineText | FieldKind::Date => {
            refresh_text(doc, field, kind)
        }
        FieldKind::Dropdown => refresh_dropdown(doc, field),
        FieldKind::List => refresh_list(doc, field),
        FieldKind::Checkbox | FieldKind::RadioGroup => {
            refresh_buttons(doc, field, kind, redraw_buttons)
        }
        FieldKind::Signature => refresh_empty_signature(doc, field),
        FieldKind::Button => Ok(()),
    }
}

/// the text of a field's /V, empty when unset.
pub fn stored_text(doc: &Document, field: &FieldNode) -> String {
    match field.inherited(doc, b"V") {
        Some(Object::String(bytes, _)) => crate::objects::decode_text_string(bytes),
        Some(Object::Name(bytes)) => String::from_utf8_lossy(bytes).into_owned(),
        _ => String::new(),
    }
}

fn refresh_text(doc: &mut Document, field: &FieldNode, kind: FieldKind) -> FormResult<()> {
    let mut text = stored_text(doc, field);
    if field.has_flag(doc, flags::PASSWORD) {
        text = "*".repeat(text.chars().count());
    }
    let comb_cells = if field.has_flag(doc, flags::COMB)
        && !field.has_flag(doc, flags::MULTILINE | flags::PASSWORD)
    {
        field.max_length(doc)
    } else {
        None
    };
    let layout = TextLayout {
        text,
        multiline: kind == FieldKind::MultilineText,
        comb_cells,
        alignment: Alignment::from_quadding(field.quadding(doc)),
    };
    draw_text_widgets(doc, field, &layout)
}

fn draw_text_widgets(doc: &mut Document, field: &FieldNode, layout: &TextLayout) -> FormResult<()> {
    let da = DefaultAppearance::parse(&field.default_appearance(doc).unwrap_or_default());
    let needs_win_ansi = !layout.text.is_ascii();
    let font = resolve_font(doc, da.font.as_deref(), needs_win_ansi)?;
    for widget_id in &field.widgets {
        let Some(widget) = WidgetBox::read(doc, *widget_id) else {
            continue;
        };
        let appearance = text_appearance(&widget, &da, &font, layout)?;
        store_appearance(doc, *widget_id, &widget, &appearance)?;
    }
    Ok(())
}

fn refresh_dropdown(doc: &mut Document, field: &FieldNode) -> FormResult<()> {
    let value = selected_values(doc, field)
        .into_iter()
        .next()
        .unwrap_or_default();
    // the box shows the choice's label, not the value stored for it
    let label = options_of(doc, field)
        .into_iter()
        .find(|(export, _)| *export == value)
        .map(|(_, label)| label)
        .unwrap_or(value);
    let layout = TextLayout {
        text: label,
        multiline: false,
        comb_cells: None,
        alignment: Alignment::from_quadding(field.quadding(doc)),
    };
    draw_text_widgets(doc, field, &layout)
}

/// the chosen values of a choice field's /V (a string or an array).
pub fn selected_values(doc: &Document, field: &FieldNode) -> Vec<String> {
    match field.inherited(doc, b"V") {
        Some(Object::Array(items)) => items
            .iter()
            .filter_map(|item| match resolve(doc, item) {
                Object::String(bytes, _) => Some(crate::objects::decode_text_string(bytes)),
                _ => None,
            })
            .collect(),
        Some(Object::String(bytes, _)) => vec![crate::objects::decode_text_string(bytes)],
        _ => Vec::new(),
    }
}

fn refresh_list(doc: &mut Document, field: &FieldNode) -> FormResult<()> {
    let options = options_of(doc, field);
    let selected = selected_values(doc, field);
    let labels: Vec<String> = options.iter().map(|(_, label)| label.clone()).collect();
    let chosen: Vec<bool> = options
        .iter()
        .map(|(export, _)| selected.contains(export))
        .collect();
    let top_index = field
        .inherited(doc, b"TI")
        .and_then(number_of)
        .unwrap_or(0.0)
        .max(0.0) as usize;
    let da = DefaultAppearance::parse(&field.default_appearance(doc).unwrap_or_default());
    let needs_win_ansi = labels.iter().any(|label| !label.is_ascii());
    let font = resolve_font(doc, da.font.as_deref(), needs_win_ansi)?;
    for widget_id in &field.widgets {
        let Some(widget) = WidgetBox::read(doc, *widget_id) else {
            continue;
        };
        let appearance = list_appearance(&widget, &da, &font, &labels, &chosen, top_index)?;
        store_appearance(doc, *widget_id, &widget, &appearance)?;
    }
    Ok(())
}

fn refresh_buttons(
    doc: &mut Document,
    field: &FieldNode,
    kind: FieldKind,
    redraw: bool,
) -> FormResult<()> {
    let value = field
        .inherited(doc, b"V")
        .and_then(|value| value.as_name().ok())
        .map(<[u8]>::to_vec)
        .unwrap_or_else(|| b"Off".to_vec());
    let da = DefaultAppearance::parse(&field.default_appearance(doc).unwrap_or_default());
    let radio = kind == FieldKind::RadioGroup;
    for widget_id in &field.widgets {
        let state = on_state(doc, *widget_id);
        if redraw || !has_state_drawings(doc, *widget_id, &state) {
            if let Some(widget) = WidgetBox::read(doc, *widget_id) {
                let caption = caption_of(doc, *widget_id);
                let style = CheckStyle::from_caption(caption.as_deref(), radio);
                let on = check_appearance(&widget, &da, style, radio, true);
                let off = check_appearance(&widget, &da, style, radio, false);
                store_state_appearances(doc, *widget_id, &widget, &state, &on, &off)?;
            }
        }
        let shown = if state == value {
            state
        } else {
            b"Off".to_vec()
        };
        doc.get_dictionary_mut(*widget_id)?
            .set("AS", Object::Name(shown));
    }
    Ok(())
}

fn caption_of(doc: &Document, widget: ObjectId) -> Option<String> {
    let dict = doc.get_dictionary(widget).ok()?;
    let characteristics = get_dict(doc, dict, b"MK")?;
    crate::objects::get_text(doc, characteristics, b"CA")
}

/// whether a widget already has drawings for both its on and off states.
fn has_state_drawings(doc: &Document, widget: ObjectId, state: &[u8]) -> bool {
    let Ok(dict) = doc.get_dictionary(widget) else {
        return false;
    };
    let Some(normal) = get_dict(doc, dict, b"AP").and_then(|ap| get(doc, ap, b"N")) else {
        return false;
    };
    match normal.as_dict() {
        Ok(states) => {
            let drawn = |key: &[u8]| {
                matches!(
                    states.get(key),
                    Ok(Object::Reference(_)) | Ok(Object::Stream(_))
                )
            };
            drawn(state) && drawn(b"Off")
        }
        Err(_) => false,
    }
}

/// an unsigned signature field gets an empty drawing (plus its frame), so
/// readers draw its box consistently.
fn refresh_empty_signature(doc: &mut Document, field: &FieldNode) -> FormResult<()> {
    if field.inherited(doc, b"V").is_some() {
        return Ok(());
    }
    for widget_id in &field.widgets {
        let has_drawing = doc
            .get_dictionary(*widget_id)
            .map(|dict| dict.has(b"AP"))
            .unwrap_or(false);
        if has_drawing {
            continue;
        }
        if let Some(widget) = WidgetBox::read(doc, *widget_id) {
            let appearance = Appearance {
                content: frame_content(&widget, false),
                fonts: Vec::new(),
            };
            store_appearance(doc, *widget_id, &widget, &appearance)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plain_box(width: f32, height: f32) -> WidgetBox {
        WidgetBox {
            width,
            height,
            matrix: Matrix::IDENTITY,
            border_width: 0.0,
            border_color: None,
            background: None,
            border_style: b'S',
        }
    }

    fn helvetica() -> AppearanceFont {
        AppearanceFont {
            resource_name: "Helv".into(),
            object: Object::Null,
            metrics: FontMetrics::standard(StandardFont::Helvetica),
        }
    }

    #[test]
    fn reads_default_appearance_strings() {
        let da = DefaultAppearance::parse(b"/Helv 12 Tf 0 0 1 rg");
        assert_eq!(da.font.as_deref(), Some("Helv"));
        assert_eq!(da.size, 12.0);
        assert_eq!(da.color, "0 0 1 rg");
        assert_eq!(da.stroke_color(), "0 0 1 RG");
        assert_eq!(DefaultAppearance::parse(b"garbage(").size, 0.0);
    }

    #[test]
    fn wraps_at_spaces_and_cuts_long_words() {
        let metrics = FontMetrics::standard(StandardFont::Courier);
        // courier at size 10 is 6 points a letter: 5 letters fit in 30 points
        let lines = wrap_lines(b"ab cd efghijkl\nx", &metrics, 10.0, 30.0);
        assert_eq!(
            lines,
            vec![
                b"ab cd".to_vec(),
                b"efghi".to_vec(),
                b"jkl".to_vec(),
                b"x".to_vec()
            ]
        );
    }

    #[test]
    fn auto_size_fits_the_box() {
        let widget = plain_box(100.0, 20.0);
        let da = DefaultAppearance::parse(b"/Helv 0 Tf 0 g");
        let layout = TextLayout {
            text: "Hello".into(),
            multiline: false,
            comb_cells: None,
            alignment: Alignment::Left,
        };
        let appearance = text_appearance(&widget, &da, &helvetica(), &layout).unwrap();
        assert!(appearance.content.contains("(Hello) Tj"));
        // (20 - 2) / 1.15 is about 15.65
        assert!(
            appearance.content.contains("/Helv 15.652 Tf"),
            "{}",
            appearance.content
        );
    }

    #[test]
    fn right_aligned_text_ends_at_the_padding() {
        let widget = plain_box(100.0, 20.0);
        let da = DefaultAppearance::parse(b"/Helv 10 Tf 0 g");
        let layout = TextLayout {
            text: "Hi".into(),
            multiline: false,
            comb_cells: None,
            alignment: Alignment::Right,
        };
        let appearance = text_appearance(&widget, &da, &helvetica(), &layout).unwrap();
        // "Hi" is 9.44 points wide at size 10, so it starts at 100 - 2 - 9.44
        assert!(
            appearance.content.contains("88.56 "),
            "{}",
            appearance.content
        );
    }

    #[test]
    fn comb_fields_put_one_letter_per_cell() {
        let widget = plain_box(100.0, 20.0);
        let da = DefaultAppearance::parse(b"/Helv 10 Tf 0 g");
        let layout = TextLayout {
            text: "ABCD".into(),
            multiline: false,
            comb_cells: Some(5),
            alignment: Alignment::Left,
        };
        let appearance = text_appearance(&widget, &da, &helvetica(), &layout).unwrap();
        assert_eq!(appearance.content.matches(" Tj").count(), 4);
    }

    #[test]
    fn text_outside_winansi_is_refused() {
        let widget = plain_box(100.0, 20.0);
        let da = DefaultAppearance::parse(b"/Helv 10 Tf 0 g");
        let layout = TextLayout {
            text: "日本".into(),
            multiline: false,
            comb_cells: None,
            alignment: Alignment::Left,
        };
        let error = text_appearance(&widget, &da, &helvetica(), &layout).unwrap_err();
        assert!(matches!(error, FormError::UnsupportedCharacters(_)));
    }

    #[test]
    fn turned_widgets_start_at_the_origin() {
        let matrix = upright_matrix(90, 100.0, 20.0);
        let moved = matrix.apply_rect(&PdfRect::from_corners(0.0, 0.0, 100.0, 20.0));
        assert_eq!(
            (moved.left, moved.bottom, moved.right, moved.top),
            (0.0, 0.0, 20.0, 100.0)
        );
    }
}

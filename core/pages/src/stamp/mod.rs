//! stamps drawn onto pages: page numbers, headers and footers, and
//! watermarks, and taking them off again.
//!
//! stamps become part of the page's drawing, so every pdf reader shows
//! and prints them. each one is tagged, so `remove_stamps` can take off
//! exactly what Letters added. positions are worked out on the page as
//! the person sees it, so a stamp lands in the same place on a turned or
//! cropped page as on a plain one.

mod image;
mod page_content;
mod text;

use lopdf::{Dictionary, Document, Object, ObjectId};
use serde::{Deserialize, Serialize};

use crate::changes::Changes;
use crate::tree::{check_selection, number, page_ids, PageFrame};
use crate::PagesError;

use page_content::{plan_removal, plan_stamp, NeededResource, StampDrawing};
use text::encode_win_ansi;
pub use text::{Rgb, StampFont};

/// what a stamp is, so one kind can be removed without touching the others.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum StampKind {
    PageNumbers,
    HeaderFooter,
    Watermark,
}

impl StampKind {
    /// the name written into the page's tag for this kind.
    fn tag_name(self) -> &'static str {
        match self {
            StampKind::PageNumbers => "PageNumbers",
            StampKind::HeaderFooter => "HeaderFooter",
            StampKind::Watermark => "Watermark",
        }
    }
}

/// whether a stamp is drawn over the page or underneath it.
///
/// a stamp underneath is hidden wherever the page itself is painted, so
/// on a scanned page (one big picture) it can't be seen at all.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum StampLayer {
    #[default]
    InFront,
    Behind,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum VerticalEdge {
    Top,
    #[default]
    Bottom,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum HorizontalPlace {
    Left,
    #[default]
    Center,
    Right,
}

/// where on the page a page number goes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StampPosition {
    pub vertical: VerticalEdge,
    pub horizontal: HorizontalPlace,
}

/// how a page number is written.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum NumberFormat {
    /// "1"
    #[default]
    Number,
    /// "Page 1"
    PageWord,
    /// "1 of 12"
    OfTotal,
    /// "i", "ii", "iii"
    Roman,
}

/// settings for `add_page_numbers`. missing fields take the defaults.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct PageNumberParams {
    pub position: StampPosition,
    /// the number on the first chosen page
    pub start_number: u32,
    pub format: NumberFormat,
    pub font: StampFont,
    pub font_size: f32,
    /// the distance from the page edges to the text, in points
    pub margin: f32,
    pub color: Rgb,
    /// the pages to number, or none for every page
    pub pages: Option<Vec<u32>>,
    /// leaves the first chosen page (often a title page) without a number.
    /// it still counts, so the next page shows the second number
    pub skip_first_page: bool,
}

impl Default for PageNumberParams {
    fn default() -> Self {
        Self {
            position: StampPosition::default(),
            start_number: 1,
            format: NumberFormat::default(),
            font: StampFont::default(),
            font_size: 10.0,
            margin: 36.0,
            color: Rgb::BLACK,
            pages: None,
            skip_first_page: false,
        }
    }
}

/// settings for `add_header_footer`. each of the three texts may use the
/// tokens {page}, {pages}, {date} and {file}; empty texts are left out.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct HeaderFooterParams {
    /// top for a header, bottom for a footer
    pub edge: VerticalEdge,
    pub left: String,
    pub center: String,
    pub right: String,
    pub font: StampFont,
    pub font_size: f32,
    pub margin: f32,
    pub color: Rgb,
    pub pages: Option<Vec<u32>>,
    /// what {file} becomes
    pub file_name: String,
    /// what {date} becomes, already written the way the person expects
    pub date: String,
}

impl Default for HeaderFooterParams {
    fn default() -> Self {
        Self {
            edge: VerticalEdge::Top,
            left: String::new(),
            center: String::new(),
            right: String::new(),
            font: StampFont::default(),
            font_size: 10.0,
            margin: 36.0,
            color: Rgb::BLACK,
            pages: None,
            file_name: String::new(),
            date: String::new(),
        }
    }
}

/// settings for `add_text_watermark`. the text is centred on the page.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct TextWatermarkParams {
    pub text: String,
    pub font: StampFont,
    pub font_size: f32,
    pub color: Rgb,
    /// 0 is invisible, 1 is solid
    pub opacity: f32,
    /// turned anticlockwise by this many degrees, as the person sees it
    pub angle_degrees: f32,
    pub layer: StampLayer,
    pub pages: Option<Vec<u32>>,
}

impl Default for TextWatermarkParams {
    fn default() -> Self {
        Self {
            text: String::new(),
            font: StampFont::HelveticaBold,
            font_size: 60.0,
            color: Rgb {
                red: 128,
                green: 128,
                blue: 128,
            },
            opacity: 0.3,
            angle_degrees: 45.0,
            layer: StampLayer::InFront,
            pages: None,
        }
    }
}

/// settings for `add_image_watermark`. the picture is centred on the page.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ImageWatermarkParams {
    /// the bytes of a png or jpeg file
    pub image: Vec<u8>,
    pub opacity: f32,
    /// 1 makes the picture as big as fits on the page, 0.5 half that
    pub scale: f32,
    pub layer: StampLayer,
    pub pages: Option<Vec<u32>>,
}

impl Default for ImageWatermarkParams {
    fn default() -> Self {
        Self {
            image: Vec::new(),
            opacity: 0.3,
            scale: 0.5,
            layer: StampLayer::InFront,
            pages: None,
        }
    }
}

/// settings for `remove_stamps`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoveStampsParams {
    pub kind: StampKind,
    /// the pages to clear, or none for every page
    #[serde(default)]
    pub pages: Option<Vec<u32>>,
}

/// adds page numbers.
///
/// the chosen pages are numbered in document order, counting from
/// `start_number`. "1 of N" uses the last number given out as N.
pub fn add_page_numbers(doc: &mut Document, params: &PageNumberParams) -> Result<(), PagesError> {
    check_text_settings(params.font_size, params.margin)?;
    let pages = page_ids(doc)?;
    let chosen = chosen_pages(params.pages.as_deref(), pages.len())?;
    let last_number = params.start_number.saturating_add(chosen.len() as u32 - 1);

    let mut changes = Changes::new(doc);
    let font = changes.add(params.font.dictionary());
    let resources = [font_resource(font)];
    let drawing = StampDrawing {
        kind: StampKind::PageNumbers,
        layer: StampLayer::InFront,
        resources: &resources,
    };
    let line = LineStyle::of(params.font, params.font_size, params.margin, params.color);
    let mut stamped = 0;
    for (count, &index) in chosen.iter().enumerate() {
        if params.skip_first_page && count == 0 {
            continue;
        }
        let shown = params.start_number.saturating_add(count as u32);
        let label = encode_win_ansi(&page_label(shown, last_number, params.format))?;
        plan_stamp(doc, pages[index], &drawing, &mut changes, |frame, names| {
            Ok(line.draw(frame, &names[0], &label, params.position))
        })?;
        stamped += 1;
    }
    if stamped > 0 {
        changes.apply(doc);
    }
    Ok(())
}

/// adds a header or a footer: up to three texts along one edge.
pub fn add_header_footer(
    doc: &mut Document,
    params: &HeaderFooterParams,
) -> Result<(), PagesError> {
    check_text_settings(params.font_size, params.margin)?;
    let slots = [
        (HorizontalPlace::Left, &params.left),
        (HorizontalPlace::Center, &params.center),
        (HorizontalPlace::Right, &params.right),
    ];
    if slots.iter().all(|(_, text)| text.trim().is_empty()) {
        return Err(PagesError::InvalidSetting(
            "Type some text for the header or footer first.".into(),
        ));
    }
    let pages = page_ids(doc)?;
    let chosen = chosen_pages(params.pages.as_deref(), pages.len())?;

    let mut changes = Changes::new(doc);
    let font = changes.add(params.font.dictionary());
    let resources = [font_resource(font)];
    let drawing = StampDrawing {
        kind: StampKind::HeaderFooter,
        layer: StampLayer::InFront,
        resources: &resources,
    };
    let line = LineStyle::of(params.font, params.font_size, params.margin, params.color);
    for &index in &chosen {
        let tokens = Tokens {
            page: index + 1,
            pages: pages.len(),
            date: &params.date,
            file: &params.file_name,
        };
        let mut texts = Vec::new();
        for (place, template) in slots {
            if !template.trim().is_empty() {
                texts.push((place, encode_win_ansi(&tokens.fill_in(template))?));
            }
        }
        plan_stamp(doc, pages[index], &drawing, &mut changes, |frame, names| {
            let position = |horizontal| StampPosition {
                vertical: params.edge,
                horizontal,
            };
            Ok(texts
                .iter()
                .map(|(place, text)| line.draw(frame, &names[0], text, position(*place)))
                .collect::<Vec<_>>()
                .join("\n"))
        })?;
    }
    changes.apply(doc);
    Ok(())
}

/// adds a line of text across the middle of the page.
pub fn add_text_watermark(
    doc: &mut Document,
    params: &TextWatermarkParams,
) -> Result<(), PagesError> {
    if params.text.trim().is_empty() {
        return Err(PagesError::InvalidSetting(
            "Type the watermark text first.".into(),
        ));
    }
    check_font_size(params.font_size)?;
    check_opacity(params.opacity)?;
    if !params.angle_degrees.is_finite() {
        return Err(PagesError::InvalidSetting(
            "Choose an angle for the watermark.".into(),
        ));
    }
    let encoded = encode_win_ansi(&params.text)?;
    let pages = page_ids(doc)?;
    let chosen = chosen_pages(params.pages.as_deref(), pages.len())?;

    let mut changes = Changes::new(doc);
    let font = changes.add(params.font.dictionary());
    let alpha = changes.add(opacity_state(params.opacity));
    let resources = [font_resource(font), alpha_resource(alpha)];
    let drawing = StampDrawing {
        kind: StampKind::Watermark,
        layer: params.layer,
        resources: &resources,
    };
    for &index in &chosen {
        plan_stamp(doc, pages[index], &drawing, &mut changes, |frame, names| {
            Ok(centred_text(frame, params, &encoded, &names[0], &names[1]))
        })?;
    }
    changes.apply(doc);
    Ok(())
}

/// adds a picture in the middle of the page.
pub fn add_image_watermark(
    doc: &mut Document,
    params: &ImageWatermarkParams,
) -> Result<(), PagesError> {
    check_opacity(params.opacity)?;
    if !(params.scale > 0.0 && params.scale <= 10.0) {
        return Err(PagesError::InvalidSetting(
            "Choose a picture size between 1% and 1,000%.".into(),
        ));
    }
    let picture = image::decode(&params.image)?;
    let pages = page_ids(doc)?;
    let chosen = chosen_pages(params.pages.as_deref(), pages.len())?;

    let mut changes = Changes::new(doc);
    let mut stream = picture.stream;
    if let Some(mask) = picture.mask {
        let mask = changes.add(mask);
        stream.dict.set("SMask", Object::Reference(mask));
    }
    let image = changes.add(stream);
    let alpha = changes.add(opacity_state(params.opacity));
    let resources = [
        NeededResource {
            category: "XObject",
            base_name: "LettersImage",
            object: image,
        },
        alpha_resource(alpha),
    ];
    let drawing = StampDrawing {
        kind: StampKind::Watermark,
        layer: params.layer,
        resources: &resources,
    };
    let picture_size = (picture.width as f32, picture.height as f32);
    for &index in &chosen {
        plan_stamp(doc, pages[index], &drawing, &mut changes, |frame, names| {
            Ok(centred_image(
                frame,
                picture_size,
                params.scale,
                &names[0],
                &names[1],
            ))
        })?;
    }
    changes.apply(doc);
    Ok(())
}

/// takes off every stamp of one kind that Letters added, and returns how
/// many pages changed. the page's own content is left exactly as it was.
pub fn remove_stamps(doc: &mut Document, params: &RemoveStampsParams) -> Result<u32, PagesError> {
    let pages = page_ids(doc)?;
    let chosen = chosen_pages(params.pages.as_deref(), pages.len())?;
    let mut changes = Changes::new(doc);
    let mut changed = 0;
    for &index in &chosen {
        if plan_removal(doc, pages[index], params.kind, &mut changes) {
            changed += 1;
        }
    }
    changes.apply(doc);
    Ok(changed)
}

/// the chosen pages in document order, or every page when none are named.
fn chosen_pages(chosen: Option<&[u32]>, page_count: usize) -> Result<Vec<usize>, PagesError> {
    let Some(chosen) = chosen else {
        if page_count == 0 {
            return Err(PagesError::NoPagesChosen);
        }
        return Ok((0..page_count).collect());
    };
    let mut positions = check_selection(chosen, page_count)?;
    positions.sort_unstable();
    Ok(positions)
}

fn check_font_size(size: f32) -> Result<(), PagesError> {
    if (1.0..=500.0).contains(&size) {
        Ok(())
    } else {
        Err(PagesError::InvalidSetting(
            "Choose a text size between 1 and 500 points.".into(),
        ))
    }
}

fn check_text_settings(size: f32, margin: f32) -> Result<(), PagesError> {
    check_font_size(size)?;
    if (0.0..=1000.0).contains(&margin) {
        Ok(())
    } else {
        Err(PagesError::InvalidSetting(
            "Choose a margin between 0 and 1,000 points.".into(),
        ))
    }
}

fn check_opacity(opacity: f32) -> Result<(), PagesError> {
    if (0.0..=1.0).contains(&opacity) {
        Ok(())
    } else {
        Err(PagesError::InvalidSetting(
            "Choose an opacity between 0% and 100%.".into(),
        ))
    }
}

fn font_resource(object: ObjectId) -> NeededResource {
    NeededResource {
        category: "Font",
        base_name: "LettersFont",
        object,
    }
}

fn alpha_resource(object: ObjectId) -> NeededResource {
    NeededResource {
        category: "ExtGState",
        base_name: "LettersAlpha",
        object,
    }
}

/// a graphics state that makes what follows see-through.
fn opacity_state(opacity: f32) -> Dictionary {
    let mut state = Dictionary::new();
    state.set("Type", Object::Name(b"ExtGState".to_vec()));
    // ca is for filling (text and pictures), CA for outlines
    state.set("ca", Object::Real(opacity));
    state.set("CA", Object::Real(opacity));
    state
}

/// the text of one page number.
fn page_label(shown: u32, last: u32, format: NumberFormat) -> String {
    match format {
        NumberFormat::Number => shown.to_string(),
        NumberFormat::PageWord => format!("Page {shown}"),
        NumberFormat::OfTotal => format!("{shown} of {last}"),
        NumberFormat::Roman => roman(shown),
    }
}

/// lower case roman numerals. there is no roman zero, and numbers past
/// 3999 need symbols no font has, so those stay as ordinary digits.
fn roman(number: u32) -> String {
    if number == 0 || number > 3999 {
        return number.to_string();
    }
    const STEPS: [(u32, &str); 13] = [
        (1000, "m"),
        (900, "cm"),
        (500, "d"),
        (400, "cd"),
        (100, "c"),
        (90, "xc"),
        (50, "l"),
        (40, "xl"),
        (10, "x"),
        (9, "ix"),
        (5, "v"),
        (4, "iv"),
        (1, "i"),
    ];
    let mut left = number;
    let mut text = String::new();
    for (value, letters) in STEPS {
        while left >= value {
            text.push_str(letters);
            left -= value;
        }
    }
    text
}

/// what the header and footer tokens stand for on one page.
struct Tokens<'a> {
    page: usize,
    pages: usize,
    date: &'a str,
    file: &'a str,
}

impl Tokens<'_> {
    /// replaces each token in one pass, so a file name that happens to
    /// contain "{page}" is printed as it is.
    fn fill_in(&self, template: &str) -> String {
        let mut text = String::new();
        let mut rest = template;
        while let Some(start) = rest.find('{') {
            text.push_str(&rest[..start]);
            let after = &rest[start..];
            let token = ["{pages}", "{page}", "{date}", "{file}"]
                .into_iter()
                .find(|token| after.starts_with(token));
            match token {
                Some(token) => {
                    text.push_str(&self.value_of(token));
                    rest = &after[token.len()..];
                }
                None => {
                    text.push('{');
                    rest = &after[1..];
                }
            }
        }
        text.push_str(rest);
        text
    }

    fn value_of(&self, token: &str) -> String {
        match token {
            "{pages}" => self.pages.to_string(),
            "{page}" => self.page.to_string(),
            "{date}" => self.date.to_owned(),
            _ => self.file.to_owned(),
        }
    }
}

/// how a line of text along an edge is drawn.
struct LineStyle {
    font: StampFont,
    size: f32,
    margin: f32,
    color: Rgb,
}

impl LineStyle {
    fn of(font: StampFont, size: f32, margin: f32, color: Rgb) -> Self {
        Self {
            font,
            size,
            margin,
            color,
        }
    }

    /// drawing instructions for one line at a place along an edge, in
    /// upright coordinates. at the top, the capitals' top edge sits at the
    /// margin; at the bottom, the baseline does.
    fn draw(
        &self,
        frame: &PageFrame,
        font_name: &str,
        text: &[u8],
        position: StampPosition,
    ) -> String {
        let width = self.font.text_width(text, self.size);
        let x = match position.horizontal {
            HorizontalPlace::Left => self.margin,
            HorizontalPlace::Center => (frame.shown_width() - width) / 2.0,
            HorizontalPlace::Right => frame.shown_width() - self.margin - width,
        };
        let y = match position.vertical {
            VerticalEdge::Bottom => self.margin,
            VerticalEdge::Top => {
                frame.shown_height() - self.margin - self.font.cap_height() * self.size / 1000.0
            }
        };
        format!(
            "BT /{font_name} {} Tf {} {} {} Td {} Tj ET",
            number(self.size),
            self.color.fill_operator(),
            number(x),
            number(y),
            text::pdf_string(text)
        )
    }
}

/// drawing instructions for watermark text, turned about its own centre,
/// which sits on the centre of the page.
fn centred_text(
    frame: &PageFrame,
    params: &TextWatermarkParams,
    text: &[u8],
    font_name: &str,
    alpha_name: &str,
) -> String {
    let width = params.font.text_width(text, params.font_size);
    let height = params.font.cap_height() * params.font_size / 1000.0;
    let (sin, cos) = params.angle_degrees.to_radians().sin_cos();
    // start the text so that, once turned, its middle lands on the page's
    let (half_width, half_height) = (width / 2.0, height / 2.0);
    let x = frame.shown_width() / 2.0 - (half_width * cos - half_height * sin);
    let y = frame.shown_height() / 2.0 - (half_width * sin + half_height * cos);
    let matrix = [cos, sin, -sin, cos, x, y].map(number).join(" ");
    format!(
        "/{alpha_name} gs BT /{font_name} {} Tf {} {matrix} Tm {} Tj ET",
        number(params.font_size),
        params.color.fill_operator(),
        text::pdf_string(text)
    )
}

/// drawing instructions for a watermark picture, kept in proportion and
/// centred on the page.
fn centred_image(
    frame: &PageFrame,
    (picture_width, picture_height): (f32, f32),
    scale: f32,
    image_name: &str,
    alpha_name: &str,
) -> String {
    let fit = (frame.shown_width() / picture_width).min(frame.shown_height() / picture_height);
    let (width, height) = (picture_width * fit * scale, picture_height * fit * scale);
    let x = (frame.shown_width() - width) / 2.0;
    let y = (frame.shown_height() - height) / 2.0;
    format!(
        "/{alpha_name} gs {} 0 0 {} {} {} cm /{image_name} Do",
        number(width),
        number(height),
        number(x),
        number(y)
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roman_numerals() {
        assert_eq!(roman(1), "i");
        assert_eq!(roman(4), "iv");
        assert_eq!(roman(14), "xiv");
        assert_eq!(roman(1999), "mcmxcix");
        assert_eq!(roman(0), "0");
    }

    #[test]
    fn page_labels_follow_the_format() {
        assert_eq!(page_label(3, 9, NumberFormat::Number), "3");
        assert_eq!(page_label(3, 9, NumberFormat::PageWord), "Page 3");
        assert_eq!(page_label(3, 9, NumberFormat::OfTotal), "3 of 9");
        assert_eq!(page_label(3, 9, NumberFormat::Roman), "iii");
    }

    #[test]
    fn tokens_are_filled_in_once() {
        let tokens = Tokens {
            page: 2,
            pages: 10,
            date: "11 Nov 2026",
            file: "{page}.pdf",
        };
        assert_eq!(
            tokens.fill_in("{file} – page {page} of {pages}, {date} {x}"),
            "{page}.pdf – page 2 of 10, 11 Nov 2026 {x}"
        );
    }
}

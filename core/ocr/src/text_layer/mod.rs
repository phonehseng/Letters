//! writing recognised words into a pdf as invisible text.
//!
//! the page keeps its picture exactly as it was. on top, each word is
//! written in "invisible" mode (text render mode 3: neither filled nor
//! outlined), placed and stretched to cover the word in the picture. a
//! reader shows the picture, but selecting, searching and copying find the
//! invisible words, so the scan behaves like a page of real text.
//!
//! the layer lives in its own content stream, wrapped in a marked-content
//! tag, so running recognition again replaces it instead of stacking a
//! second copy on top.

mod fonts;
mod glyphless;
mod helvetica;
mod winansi;

use lopdf::content::{Content, Operation};
use lopdf::{dictionary, Dictionary, Document, Object, ObjectId, Stream, StringFormat};

use crate::geometry::{Affine, Point};
use crate::pdf_page::{inherited, page_id, PageSpace};
use crate::result::{OcrPage, OcrWord};
use crate::OcrError;

use fonts::{
    any_script_font, compressed, encode_any_script, latin_font, ANY_SCRIPT_FONT_NAME,
    LATIN_FONT_NAME, MARKER_KEY,
};

pub(crate) use fonts::compressed as compressed_stream;

/// the marked-content tag around the layer's drawing instructions. other
/// tools can find (or skip) the layer by it.
pub const LAYER_TAG: &[u8] = b"LettersOcrText";

/// what the stream dictionaries this module adds are marked with.
const LAYER_MARK: &[u8] = b"TextLayer";
const OPEN_WRAP_MARK: &[u8] = b"WrapOpen";
const CLOSE_WRAP_MARK: &[u8] = b"WrapClose";

/// adds (or replaces) the invisible text layer of the zero based `page`.
///
/// `words` must be measured on the page as shown, as recognition gives
/// them. if the recognised picture was drawn at another size, the words
/// are scaled to the page.
pub fn add_text_layer(doc: &mut Document, page: u32, words: &OcrPage) -> Result<(), OcrError> {
    let page_id = page_id(doc, page)?;
    let space = PageSpace::of_page(doc, page_id)?;
    remove_layer_streams(doc, page_id)?;
    if words.words.is_empty() {
        return Ok(());
    }

    let scale = scale_to_page(words, &space);
    let latin = latin_font(doc);
    let needs_any_script = words
        .words
        .iter()
        .any(|word| winansi::encode(&word.text).is_none());
    let any_script = needs_any_script.then(|| any_script_font(doc));
    add_fonts_to_resources(doc, page_id, latin, any_script)?;

    let operations = layer_operations(&words.words, &scale.then(&space.to_user));
    let bytes = Content { operations }
        .encode()
        .map_err(|error| OcrError::Damaged(error.to_string()))?;
    let layer = doc.add_object(compressed(marked(LAYER_MARK), &bytes));
    append_wrapped(doc, page_id, layer)
}

/// removes the invisible text layer from a page, if it has one. returns
/// whether there was one.
pub fn remove_text_layer(doc: &mut Document, page: u32) -> Result<bool, OcrError> {
    let page_id = page_id(doc, page)?;
    remove_layer_streams(doc, page_id)
}

/// whether the page carries a text layer written by this module.
pub fn has_text_layer(doc: &Document, page: u32) -> Result<bool, OcrError> {
    let page_id = page_id(doc, page)?;
    Ok(content_ids(doc, page_id)
        .iter()
        .any(|&id| stream_mark(doc, id).as_deref() == Some(LAYER_MARK)))
}

/// stretches recognised positions to the shown page, for pictures drawn at
/// a slightly different size than the page.
fn scale_to_page(words: &OcrPage, space: &PageSpace) -> Affine {
    let across = if words.width > 0.0 {
        space.width / words.width
    } else {
        1.0
    };
    let down = if words.height > 0.0 {
        space.height / words.height
    } else {
        1.0
    };
    Affine::scale(across, down)
}

/// the drawing instructions: one invisible piece of text per word.
fn layer_operations(words: &[OcrWord], to_user: &Affine) -> Vec<Operation> {
    let mut operations = vec![
        Operation::new("BMC", vec![Object::Name(LAYER_TAG.to_vec())]),
        Operation::new("BT", vec![]),
        // render mode 3: draw nothing, but the text is still text
        Operation::new("Tr", vec![3.into()]),
    ];
    for word in words {
        if let Some(placed) = place_word(word, to_user) {
            operations.extend(placed);
        }
    }
    operations.push(Operation::new("ET", vec![]));
    operations.push(Operation::new("EMC", vec![]));
    operations
}

/// sets the font, stretch and position for one word, then shows it.
///
/// the text matrix's first column is the direction the word runs in user
/// space and its second the direction "up" for its letters, so a word on a
/// crooked line, or on a turned page, lies exactly along its baseline.
fn place_word(word: &OcrWord, to_user: &Affine) -> Option<Vec<Operation>> {
    let start = to_user.apply(word.baseline.start);
    let end = to_user.apply(word.baseline.end);
    let along = Point::new(end.x - start.x, end.y - start.y);
    let length = (along.x * along.x + along.y * along.y).sqrt();
    // the font size is scaled the same way as the page, measured upward
    let (up, upward_scale) = upward(word, to_user)?;
    let font_size = word.font_size * upward_scale;
    if length < 0.01 || font_size < 0.01 {
        return None;
    }
    // widths in thousandths of the font size
    let (font, text, natural_width) = match winansi::encode(&word.text) {
        Some(codes) => {
            let width = helvetica::text_width(&codes);
            (LATIN_FONT_NAME, codes, width)
        }
        None => {
            let codes = encode_any_script(&word.text);
            let width = (codes.len() / 2) as u32 * u32::from(glyphless::GLYPH_WIDTH);
            (ANY_SCRIPT_FONT_NAME, codes, width)
        }
    };
    let natural_width = natural_width as f32 / 1000.0 * font_size;
    if natural_width <= 0.0 {
        return None;
    }
    let stretch = 100.0 * length / natural_width;
    let direction = Point::new(along.x / length, along.y / length);
    Some(vec![
        Operation::new("Tf", vec![Object::Name(font.to_vec()), font_size.into()]),
        Operation::new("Tz", vec![stretch.into()]),
        Operation::new(
            "Tm",
            vec![
                direction.x.into(),
                direction.y.into(),
                up.x.into(),
                up.y.into(),
                start.x.into(),
                start.y.into(),
            ],
        ),
        Operation::new("Tj", vec![Object::String(text, StringFormat::Hexadecimal)]),
    ])
}

/// the user space direction that is "up" for the word's letters, as a
/// unit vector, and how much the map stretches that direction.
fn upward(word: &OcrWord, to_user: &Affine) -> Option<(Point, f32)> {
    let along = Point::new(
        word.baseline.end.x - word.baseline.start.x,
        word.baseline.end.y - word.baseline.start.y,
    );
    let length = word.baseline.length();
    if length < 1e-6 {
        return None;
    }
    // on the shown page y grows downward, so "up" is the run direction
    // turned a quarter anticlockwise: (x, y) -> (y, -x)
    let shown_up = Point::new(along.y / length, -along.x / length);
    let user_up = to_user.apply_to_vector(shown_up);
    let stretch = (user_up.x * user_up.x + user_up.y * user_up.y).sqrt();
    (stretch > 1e-6).then(|| {
        (
            Point::new(user_up.x / stretch, user_up.y / stretch),
            stretch,
        )
    })
}

/// a stream dictionary carrying this module's marker.
fn marked(mark: &[u8]) -> Dictionary {
    dictionary! { MARKER_KEY => Object::Name(mark.to_vec()) }
}

/// the marker of a content stream, if this module wrote it.
fn stream_mark(doc: &Document, id: ObjectId) -> Option<Vec<u8>> {
    let stream = doc.get_object(id).ok()?.as_stream().ok()?;
    Some(stream.dict.get(MARKER_KEY).ok()?.as_name().ok()?.to_vec())
}

/// the content streams of a page in drawing order, whether /Contents is
/// one stream or an array of them.
fn content_ids(doc: &Document, page_id: ObjectId) -> Vec<ObjectId> {
    doc.get_page_contents(page_id)
}

/// takes the layer's streams out of the page. returns whether any was there.
///
/// streams this module wrote are recognised by their marker. a stream
/// someone else wrote that still holds the tagged instructions (another
/// tool may have merged the page's streams into one) has just those
/// instructions cut out.
fn remove_layer_streams(doc: &mut Document, page_id: ObjectId) -> Result<bool, OcrError> {
    let mut kept = Vec::new();
    let mut found = false;
    for id in content_ids(doc, page_id) {
        if stream_mark(doc, id).as_deref() == Some(LAYER_MARK) {
            doc.objects.remove(&id);
            found = true;
        } else {
            found |= cut_tagged_instructions(doc, id)?;
            kept.push(id);
        }
    }
    if found {
        set_contents(doc, page_id, kept)?;
    }
    Ok(found)
}

/// removes everything between our tag and its end from one stream.
fn cut_tagged_instructions(doc: &mut Document, id: ObjectId) -> Result<bool, OcrError> {
    let Ok(Object::Stream(stream)) = doc.get_object(id) else {
        return Ok(false);
    };
    let Ok(bytes) = stream.decompressed_content() else {
        return Ok(false);
    };
    // a quick look before parsing: almost every stream won't have the tag
    if !contains(&bytes, LAYER_TAG) {
        return Ok(false);
    }
    let Ok(content) = Content::decode(&bytes) else {
        return Ok(false);
    };
    let (operations, cut) = without_tagged(content.operations);
    if !cut {
        return Ok(false);
    }
    let encoded = Content { operations }
        .encode()
        .map_err(|error| OcrError::Damaged(error.to_string()))?;
    if let Ok(Object::Stream(stream)) = doc.get_object_mut(id) {
        stream.dict.remove(b"Filter");
        stream.dict.remove(b"DecodeParms");
        stream.set_plain_content(encoded);
    }
    Ok(true)
}

/// the instructions without those inside our marked-content tag.
fn without_tagged(operations: Vec<Operation>) -> (Vec<Operation>, bool) {
    let mut kept = Vec::with_capacity(operations.len());
    // how deeply nested we are inside our tag; 0 means outside it
    let mut depth = 0usize;
    let mut cut = false;
    for operation in operations {
        let opens = matches!(operation.operator.as_str(), "BMC" | "BDC");
        if depth > 0 {
            if opens {
                depth += 1;
            } else if operation.operator == "EMC" {
                depth -= 1;
            }
            continue;
        }
        let is_ours = opens
            && operation
                .operands
                .first()
                .and_then(|tag| tag.as_name().ok())
                == Some(LAYER_TAG);
        if is_ours {
            depth = 1;
            cut = true;
            continue;
        }
        kept.push(operation);
    }
    (kept, cut)
}

/// puts the layer last, after the page's own content wrapped in q and Q.
///
/// the wrap matters: a page's own content may leave the drawing state
/// changed at its end (a transformation, say), which would move every
/// word. q saves the state before the page's content and Q puts it back.
/// the wrap is added once and reused on later runs.
fn append_wrapped(doc: &mut Document, page_id: ObjectId, layer: ObjectId) -> Result<(), OcrError> {
    let mut contents = content_ids(doc, page_id);
    let already_wrapped = contents
        .first()
        .is_some_and(|&id| stream_mark(doc, id).as_deref() == Some(OPEN_WRAP_MARK))
        && contents
            .last()
            .is_some_and(|&id| stream_mark(doc, id).as_deref() == Some(CLOSE_WRAP_MARK));
    if !already_wrapped && !contents.is_empty() {
        let open = doc.add_object(Stream::new(marked(OPEN_WRAP_MARK), b"q\n".to_vec()));
        let close = doc.add_object(Stream::new(marked(CLOSE_WRAP_MARK), b"\nQ\n".to_vec()));
        contents.insert(0, open);
        contents.push(close);
    }
    contents.push(layer);
    set_contents(doc, page_id, contents)
}

fn set_contents(
    doc: &mut Document,
    page_id: ObjectId,
    contents: Vec<ObjectId>,
) -> Result<(), OcrError> {
    let page = doc
        .get_dictionary_mut(page_id)
        .map_err(|_| OcrError::Damaged("a page is missing".into()))?;
    let references: Vec<Object> = contents.into_iter().map(Object::Reference).collect();
    page.set("Contents", references);
    Ok(())
}

/// makes the fonts available to the page under their resource names.
///
/// the page gets its own copy of its resources (and of their font list)
/// if it shared or inherited them, so other pages are left as they were.
fn add_fonts_to_resources(
    doc: &mut Document,
    page_id: ObjectId,
    latin: ObjectId,
    any_script: Option<ObjectId>,
) -> Result<(), OcrError> {
    let page = doc
        .get_dictionary(page_id)
        .map_err(|_| OcrError::Damaged("a page is missing".into()))?;
    let mut resources = inherited(doc, page, b"Resources")
        .and_then(|object| resolve_dict(doc, object))
        .unwrap_or_default();
    let mut fonts = resources
        .get(b"Font")
        .ok()
        .and_then(|object| resolve_dict(doc, object))
        .unwrap_or_default();
    fonts.set(LATIN_FONT_NAME, latin);
    if let Some(any_script) = any_script {
        fonts.set(ANY_SCRIPT_FONT_NAME, any_script);
    }
    resources.set("Font", fonts);
    let page = doc
        .get_dictionary_mut(page_id)
        .map_err(|_| OcrError::Damaged("a page is missing".into()))?;
    page.set("Resources", resources);
    Ok(())
}

fn resolve_dict(doc: &Document, object: &Object) -> Option<Dictionary> {
    let (_, object) = doc.dereference(object).ok()?;
    object.as_dict().ok().cloned()
}

fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    haystack
        .windows(needle.len())
        .any(|window| window == needle)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::Rect;
    use crate::result::Baseline;

    fn word(text: &str, x: f32, baseline_y: f32, width: f32) -> OcrWord {
        OcrWord {
            text: text.into(),
            rect: Rect::new(x, baseline_y - 10.0, width, 12.0),
            baseline: Baseline {
                start: Point::new(x, baseline_y),
                end: Point::new(x + width, baseline_y),
            },
            font_size: 12.0,
            confidence: 95.0,
            line: 0,
            paragraph: 0,
            block: 0,
            language: None,
        }
    }

    fn page_of(words: Vec<OcrWord>) -> OcrPage {
        OcrPage {
            width: 612.0,
            height: 792.0,
            words,
            language: "eng".into(),
            skew_angle: 0.0,
            orientation: 0,
            mean_confidence: 95.0,
            steps: Vec::new(),
        }
    }

    /// a one page document with a little content of its own.
    fn document(rotate: i64) -> Document {
        let mut doc = Document::with_version("1.7");
        let pages_id = doc.new_object_id();
        let content = doc.add_object(Stream::new(
            Dictionary::new(),
            b"0 0 1 rg 0 0 10 10 re f".to_vec(),
        ));
        let page_id = doc.add_object(dictionary! {
            "Type" => "Page",
            "Parent" => pages_id,
            "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
            "Rotate" => rotate,
            "Contents" => content,
        });
        doc.objects.insert(
            pages_id,
            dictionary! { "Type" => "Pages", "Kids" => vec![page_id.into()], "Count" => 1 }.into(),
        );
        let catalog = doc.add_object(dictionary! { "Type" => "Catalog", "Pages" => pages_id });
        doc.trailer.set("Root", catalog);
        doc
    }

    fn layer_text(doc: &Document) -> String {
        let page_id = page_id(doc, 0).unwrap();
        let ids = content_ids(doc, page_id);
        let layer = ids.last().unwrap();
        let stream = doc.get_object(*layer).unwrap().as_stream().unwrap();
        String::from_utf8_lossy(&stream.decompressed_content().unwrap()).into_owned()
    }

    #[test]
    fn words_are_stretched_to_their_box() {
        let mut doc = document(0);
        // "Hello" in 12 point helvetica is 27.336 points wide, so a 60
        // point box needs a stretch of 219.49 percent
        add_text_layer(
            &mut doc,
            0,
            &page_of(vec![word("Hello", 72.0, 100.0, 60.0)]),
        )
        .unwrap();
        let text = layer_text(&doc);
        assert!(text.contains("/LettersOcrText BMC"), "{text}");
        assert!(text.contains("3 Tr"), "{text}");
        let stretch: f32 = text
            .lines()
            .find_map(|line| line.strip_suffix(" Tz"))
            .unwrap()
            .parse()
            .unwrap();
        assert!((stretch - 219.49).abs() < 0.01, "{text}");
        // shown y 100 is user y 692 on a letter page
        assert!(text.contains("1.0 0.0 0.0 1.0 72.0 692.0 Tm"), "{text}");
        assert!(text.contains("<48656C6C6F> Tj"), "{text}");
    }

    #[test]
    fn on_a_turned_page_the_text_runs_along_the_shown_line() {
        let mut doc = document(90);
        let mut words = page_of(vec![word("Hi", 10.0, 20.0, 12.0)]);
        // the shown page is the stored one turned on its side
        (words.width, words.height) = (792.0, 612.0);
        add_text_layer(&mut doc, 0, &words).unwrap();
        let text = layer_text(&doc);
        // a quarter turn: the word runs up the stored page and its letters
        // point left; shown (10, 20) is stored (20, 10)
        assert!(text.contains("0.0 1.0 -1.0 0.0 20.0 10.0 Tm"), "{text}");
    }

    #[test]
    fn running_again_replaces_the_layer() {
        let mut doc = document(0);
        let words = page_of(vec![word("one", 72.0, 100.0, 30.0)]);
        add_text_layer(&mut doc, 0, &words).unwrap();
        let after_first = content_ids(&doc, page_id(&doc, 0).unwrap());
        add_text_layer(&mut doc, 0, &words).unwrap();
        let after_second = content_ids(&doc, page_id(&doc, 0).unwrap());
        // the wrap and the page's own content stay, one layer replaces the other
        assert_eq!(after_first.len(), 4);
        assert_eq!(after_second.len(), 4);
        assert_eq!(after_first[..3], after_second[..3]);
        assert!(has_text_layer(&doc, 0).unwrap());
        assert!(remove_text_layer(&mut doc, 0).unwrap());
        assert!(!has_text_layer(&doc, 0).unwrap());
    }

    #[test]
    fn non_western_words_use_the_any_script_font() {
        let mut doc = document(0);
        add_text_layer(&mut doc, 0, &page_of(vec![word("łódź", 72.0, 100.0, 40.0)])).unwrap();
        let text = layer_text(&doc);
        assert!(text.contains("/LettersOcrAny 12.0 Tf"), "{text}");
        assert!(text.contains("<014200F30064017A> Tj"), "{text}");
    }

    #[test]
    fn a_tag_merged_into_another_stream_is_cut_out() {
        let operations =
            Content::decode(b"q 1 0 0 1 0 0 cm Q /LettersOcrText BMC BT 3 Tr ET EMC 0 g").unwrap();
        let (kept, cut) = without_tagged(operations.operations);
        assert!(cut);
        let operators: Vec<&str> = kept
            .iter()
            .map(|operation| operation.operator.as_str())
            .collect();
        assert_eq!(operators, ["q", "cm", "Q", "g"]);
    }
}

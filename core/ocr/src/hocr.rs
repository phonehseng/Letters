//! reading tesseract's hocr output.
//!
//! hocr is html with the layout written into `class` and `title`
//! attributes. tesseract writes it very regularly, one element per line:
//!
//! ```text
//! <div class='ocr_carea' title="bbox 98 66 918 661">            a block
//!  <p class='ocr_par' lang='eng' title="bbox 98 66 918 661">     a paragraph
//!   <span class='ocr_line' title="bbox 105 66 823 113; baseline 0.015 -18; x_size 39">
//!    <span class='ocrx_word' title='bbox 105 66 178 97; x_wconf 96'>The</span>
//! ```
//!
//! hocr is used rather than tesseract's tsv because only hocr carries the
//! baseline (where the bottoms of the letters sit) and the font size.
//! this reader understands exactly that much html and no more; every
//! position stays in the pixels of the picture tesseract read.

/// a box in pixels: left, top, right, bottom.
pub type PixelBox = [f32; 4];

#[derive(Clone, Debug, Default, PartialEq)]
pub struct HocrPage {
    pub lines: Vec<HocrLine>,
    pub words: Vec<HocrWord>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct HocrLine {
    pub bbox: PixelBox,
    /// the baseline as `y = slope * (x - left) + offset`, measured from the
    /// line box's bottom left corner, the way hocr states it
    pub baseline_slope: f32,
    pub baseline_offset: f32,
    /// the height of the line's letters from the tops of tall letters to
    /// the bottoms of letters like "g", in pixels
    pub font_size: Option<f32>,
    pub block: usize,
    pub paragraph: usize,
}

impl HocrLine {
    /// the height of the baseline at `x`, in picture pixels.
    pub fn baseline_at(&self, x: f32) -> f32 {
        self.bbox[3] + self.baseline_slope * (x - self.bbox[0]) + self.baseline_offset
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct HocrWord {
    pub text: String,
    pub bbox: PixelBox,
    /// 0 to 100
    pub confidence: f32,
    /// which entry of `HocrPage::lines` the word sits on
    pub line: usize,
    /// the language tesseract thinks the word's paragraph is in
    pub language: Option<String>,
}

/// what the reader is currently inside of.
#[derive(Default)]
struct Position {
    /// the class of every element opened and not yet closed
    open: Vec<Option<String>>,
    block: usize,
    paragraph: usize,
    language: Option<String>,
    /// the word being read: its box, confidence and text so far
    word: Option<(PixelBox, f32, String)>,
}

/// reads a whole hocr document. elements it does not know are skipped.
pub fn parse(hocr: &str) -> HocrPage {
    let mut page = HocrPage::default();
    let mut position = Position::default();
    let mut rest = hocr;
    while !rest.is_empty() {
        match rest.find('<') {
            Some(0) => {
                let (tag, after) = read_tag(rest);
                rest = after;
                if let Some(tag) = tag {
                    handle_tag(tag, &mut position, &mut page);
                }
            }
            Some(start) => {
                add_text(&rest[..start], &mut position);
                rest = &rest[start..];
            }
            None => {
                add_text(rest, &mut position);
                rest = "";
            }
        }
    }
    page
}

fn add_text(text: &str, position: &mut Position) {
    if let Some((_, _, word)) = &mut position.word {
        word.push_str(&decode_entities(text));
    }
}

/// one html tag: its name, whether it closes an element, and the three
/// attributes hocr uses.
#[derive(Debug, Default, PartialEq)]
struct Tag {
    name: String,
    closing: bool,
    self_closing: bool,
    class: Option<String>,
    title: Option<String>,
    lang: Option<String>,
}

/// reads the tag at the start of `text` and returns it with what follows.
/// quotes are respected, so a `>` inside an attribute does not end the tag.
fn read_tag(text: &str) -> (Option<Tag>, &str) {
    let mut quote = None;
    let mut end = None;
    for (index, letter) in text.char_indices().skip(1) {
        match (quote, letter) {
            (None, '"' | '\'') => quote = Some(letter),
            (Some(open), _) if letter == open => quote = None,
            (None, '>') => {
                end = Some(index);
                break;
            }
            _ => {}
        }
    }
    let Some(end) = end else {
        return (None, "");
    };
    let inside = &text[1..end];
    let after = &text[end + 1..];
    // comments, the doctype and the xml declaration carry nothing for us
    if inside.starts_with('!') || inside.starts_with('?') {
        return (None, after);
    }
    (Some(parse_tag(inside)), after)
}

fn parse_tag(inside: &str) -> Tag {
    let mut tag = Tag {
        closing: inside.starts_with('/'),
        self_closing: inside.ends_with('/'),
        ..Tag::default()
    };
    let inside = inside.trim_start_matches('/').trim_end_matches('/');
    let name_end = inside
        .find(|letter: char| letter.is_whitespace())
        .unwrap_or(inside.len());
    tag.name = inside[..name_end].to_ascii_lowercase();
    let mut rest = &inside[name_end..];
    while let Some((name, value, after)) = next_attribute(rest) {
        let value = Some(decode_entities(value));
        match name.as_str() {
            "class" => tag.class = value,
            "title" => tag.title = value,
            "lang" => tag.lang = value,
            _ => {}
        }
        rest = after;
    }
    tag
}

/// reads `name='value'` (or with double quotes) from the start of `text`.
fn next_attribute(text: &str) -> Option<(String, &str, &str)> {
    let text = text.trim_start();
    let equals = text.find('=')?;
    let name = text[..equals].trim().to_ascii_lowercase();
    let after_equals = text[equals + 1..].trim_start();
    let quote = after_equals.chars().next()?;
    if quote != '\'' && quote != '"' {
        return None;
    }
    let value_end = after_equals[1..].find(quote)? + 1;
    Some((
        name,
        &after_equals[1..value_end],
        &after_equals[value_end + 1..],
    ))
}

const LINE_CLASSES: [&str; 4] = ["ocr_line", "ocr_header", "ocr_caption", "ocr_textfloat"];

fn handle_tag(tag: Tag, position: &mut Position, page: &mut HocrPage) {
    if tag.closing {
        let closed = position.open.pop().flatten();
        if closed.as_deref() == Some("ocrx_word") {
            finish_word(position, page);
        }
        return;
    }
    let class = tag.class.clone();
    match class.as_deref() {
        Some("ocr_carea") => position.block += 1,
        Some("ocr_par") => {
            position.paragraph += 1;
            position.language = tag.lang.clone();
        }
        Some(line) if LINE_CLASSES.contains(&line) => {
            if let Some(line) = read_line(tag.title.as_deref(), position) {
                page.lines.push(line);
            }
        }
        Some("ocrx_word") => {
            let title = tag.title.as_deref().unwrap_or("");
            let bbox = property(title, "bbox").and_then(|values| four(&values));
            let confidence = property(title, "x_wconf")
                .and_then(|values| values.first().copied())
                .unwrap_or(0.0);
            position.word = bbox.map(|bbox| (bbox, confidence, String::new()));
        }
        _ => {}
    }
    if !tag.self_closing {
        position.open.push(class);
    }
}

fn read_line(title: Option<&str>, position: &Position) -> Option<HocrLine> {
    let title = title?;
    let bbox = four(&property(title, "bbox")?)?;
    let baseline = property(title, "baseline").unwrap_or_default();
    Some(HocrLine {
        bbox,
        baseline_slope: baseline.first().copied().unwrap_or(0.0),
        baseline_offset: baseline.get(1).copied().unwrap_or(0.0),
        font_size: property(title, "x_size").and_then(|values| values.first().copied()),
        block: position.block,
        paragraph: position.paragraph,
    })
}

fn finish_word(position: &mut Position, page: &mut HocrPage) {
    let Some((bbox, confidence, text)) = position.word.take() else {
        return;
    };
    let text = text.trim().to_owned();
    // a word before any line would have nowhere to sit
    if text.is_empty() || page.lines.is_empty() {
        return;
    }
    page.words.push(HocrWord {
        text,
        bbox,
        confidence,
        line: page.lines.len() - 1,
        language: position.language.clone(),
    });
}

/// the numbers after `name` in a title like "bbox 1 2 3 4; x_wconf 96".
fn property(title: &str, name: &str) -> Option<Vec<f32>> {
    title.split(';').find_map(|part| {
        let mut words = part.split_whitespace();
        if words.next()? != name {
            return None;
        }
        Some(words.filter_map(|word| word.parse().ok()).collect())
    })
}

fn four(values: &[f32]) -> Option<PixelBox> {
    values.get(..4)?.try_into().ok()
}

/// turns html character references back into characters.
fn decode_entities(text: &str) -> String {
    if !text.contains('&') {
        return text.to_owned();
    }
    let mut decoded = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(start) = rest.find('&') {
        decoded.push_str(&rest[..start]);
        let after = &rest[start..];
        let Some(end) = after.find(';').filter(|&end| end <= 10) else {
            decoded.push('&');
            rest = &after[1..];
            continue;
        };
        match entity(&after[1..end]) {
            Some(letter) => {
                decoded.push(letter);
                rest = &after[end + 1..];
            }
            None => {
                decoded.push('&');
                rest = &after[1..];
            }
        }
    }
    decoded.push_str(rest);
    decoded
}

fn entity(name: &str) -> Option<char> {
    match name {
        "amp" => Some('&'),
        "lt" => Some('<'),
        "gt" => Some('>'),
        "quot" => Some('"'),
        "apos" => Some('\''),
        _ => {
            let number = name.strip_prefix('#')?;
            let code = match number.strip_prefix(['x', 'X']) {
                Some(hex) => u32::from_str_radix(hex, 16).ok()?,
                None => number.parse().ok()?,
            };
            char::from_u32(code)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE html PUBLIC "-//W3C//DTD XHTML 1.0 Transitional//EN"
    "http://www.w3.org/TR/xhtml1/DTD/xhtml1-transitional.dtd">
<html xmlns="http://www.w3.org/1999/xhtml" xml:lang="en" lang="en">
 <head><meta name='ocr-system' content='tesseract 5.5.4' /></head>
 <body>
  <div class='ocr_page' id='page_1' title='image "a>b.png"; bbox 0 0 1024 800; ppageno 0'>
   <div class='ocr_carea' id='block_1_1' title="bbox 98 66 918 661">
    <p class='ocr_par' id='par_1_1' lang='spa' title="bbox 98 66 918 661">
     <span class='ocr_line' id='line_1_1' title="bbox 105 66 823 113; baseline 0.015 -18; x_size 39; x_descenders 7; x_ascenders 9">
      <span class='ocrx_word' id='word_1_1' title='bbox 105 66 178 97; x_wconf 96'>El</span>
      <span class='ocrx_word' id='word_1_2' title='bbox 205 67 347 106; x_wconf 91'>&lt;niño&gt;</span>
     </span>
     <span class='ocr_header' id='line_1_2' title="bbox 104 115 887 165; baseline 0 -9; x_size 42">
      <span class='ocrx_word' id='word_1_3' title='bbox 104 115 199 147; x_wconf 88'>A&amp;B</span>
     </span>
    </p>
   </div>
   <div class='ocr_carea' id='block_1_2' title="bbox 98 700 918 761">
    <p class='ocr_par' id='par_1_2' lang='eng' title="bbox 98 700 918 761">
     <span class='ocr_line' id='line_1_3' title="bbox 98 700 300 761; baseline 0 0; x_size 30">
      <span class='ocrx_word' id='word_1_4' title='bbox 98 700 300 761; x_wconf 70'>end</span>
     </span>
    </p>
   </div>
  </div>
 </body>
</html>"#;

    #[test]
    fn reads_words_lines_and_structure() {
        let page = parse(SAMPLE);
        let texts: Vec<&str> = page.words.iter().map(|word| word.text.as_str()).collect();
        assert_eq!(texts, ["El", "<niño>", "A&B", "end"]);
        assert_eq!(page.lines.len(), 3);
        assert_eq!(page.words[1].bbox, [205.0, 67.0, 347.0, 106.0]);
        assert_eq!(page.words[1].confidence, 91.0);
        assert_eq!(page.words[2].line, 1);
        assert_eq!(page.words[0].language.as_deref(), Some("spa"));
        assert_eq!(page.words[3].language.as_deref(), Some("eng"));
        // the two paragraphs are in different blocks
        assert_eq!((page.lines[0].block, page.lines[0].paragraph), (1, 1));
        assert_eq!((page.lines[2].block, page.lines[2].paragraph), (2, 2));
    }

    #[test]
    fn reads_the_baseline_and_font_size() {
        let page = parse(SAMPLE);
        let line = &page.lines[0];
        assert_eq!(line.font_size, Some(39.0));
        // at the line's left edge the baseline is 18 pixels above its bottom
        assert_eq!(line.baseline_at(105.0), 113.0 - 18.0);
        assert!((line.baseline_at(205.0) - (113.0 + 1.5 - 18.0)).abs() < 1e-4);
    }

    #[test]
    fn character_references_are_decoded() {
        assert_eq!(
            decode_entities("a &amp; b &#233; &#x263A; &bogus"),
            "a & b é ☺ &bogus"
        );
    }

    #[test]
    fn empty_and_broken_input_give_an_empty_page() {
        assert_eq!(parse(""), HocrPage::default());
        assert_eq!(
            parse("<span class='ocrx_word' title='bbox 1 2"),
            HocrPage::default()
        );
    }
}

//! text boxes: upright text on every page layout, wrapping, styling and
//! characters the built-in fonts can't draw.

mod support;

use letters_annotate::{
    add_text_box, list, AnnotateError, AnnotationDetails, Color, Rect, StandardFont, TextAlign,
    TextBorder, TextBoxParams,
};
use lopdf::Document;
use support::*;

const SKY: Color = Color::new(200, 220, 255);
const RED: Color = Color::new(255, 0, 0);

fn text_box(rect: Rect, text: &str) -> TextBoxParams {
    TextBoxParams {
        rect,
        text: text.into(),
        font: StandardFont::Helvetica,
        font_size: 12.0,
        color: Color::BLACK,
        align: TextAlign::Left,
        border: None,
        fill: None,
        opacity: 1.0,
        author: None,
    }
}

fn count(
    image: &Image,
    left: u32,
    top: u32,
    right: u32,
    bottom: u32,
    test: impl Fn([u8; 3]) -> bool,
) -> usize {
    (top..bottom)
        .flat_map(|y| (left..right).map(move |x| (x, y)))
        .filter(|(x, y)| test(image.pixel(*x as f32, *y as f32)))
        .count()
}

/// the text of an annotation's appearance stream, uncompressed.
fn appearance_text(doc: &Document, annotation: &lopdf::Dictionary) -> String {
    let normal = annotation
        .get(b"AP")
        .unwrap()
        .as_dict()
        .unwrap()
        .get(b"N")
        .unwrap()
        .as_reference()
        .unwrap();
    let stream = doc.get_object(normal).unwrap().as_stream().unwrap();
    String::from_utf8(
        stream
            .decompressed_content()
            .unwrap_or_else(|_| stream.content.clone()),
    )
    .unwrap()
}

#[test]
fn text_boxes_stay_upright_on_every_page_layout() {
    let rect = Rect::new(100.0, 150.0, 200.0, 60.0);
    for (layout, page) in page_variants() {
        // the same page layouts, without the test text in the way
        let blank = page.clone().with_content("");
        let mut doc = load(&pdf_bytes(&[blank], false));
        let mut params = text_box(rect, "MMMM MMMM");
        params.font = StandardFont::HelveticaBold;
        params.font_size = 20.0;
        params.fill = Some(SKY);
        params.border = Some(TextBorder {
            color: RED,
            width: 2.0,
        });
        let info = add_text_box(&mut doc, 0, &params).unwrap();
        let saved = save(&mut doc);
        let reloaded = load(&saved);

        let annotation = annotation_by_name(&reloaded, 0, &info.id);
        assert_eq!(name_of(&annotation, b"Subtype"), "FreeText", "{layout}");
        assert_eq!(
            annotation.get(b"Contents").unwrap().as_str().unwrap(),
            b"MMMM MMMM"
        );
        let default_appearance =
            String::from_utf8_lossy(annotation.get(b"DA").unwrap().as_str().unwrap()).into_owned();
        assert!(
            default_appearance.starts_with("/HeBo 20 Tf 0 0 0 rg"),
            "{layout}: {default_appearance}"
        );
        assert_eq!(
            annotation.get(b"Rotate").unwrap().as_i64().unwrap(),
            page.rotate,
            "{layout}"
        );
        assert!(has_normal_appearance(&reloaded, &annotation));

        let listed = list(&reloaded, 0).unwrap();
        let shown = listed[0].rect;
        assert!(
            (shown.x - 100.0).abs() < 0.01 && (shown.y - 150.0).abs() < 0.01,
            "{layout}: {shown:?}"
        );
        assert!((shown.width - 200.0).abs() < 0.01 && (shown.height - 60.0).abs() < 0.01);
        let AnnotationDetails::TextBox {
            text,
            font,
            font_size,
            fill,
            border,
            ..
        } = &listed[0].details
        else {
            panic!("{layout}: wrong details");
        };
        assert_eq!(
            (text.as_str(), *font, *font_size),
            ("MMMM MMMM", StandardFont::HelveticaBold, 20.0)
        );
        assert_eq!(
            (*fill, border.map(|border| border.width)),
            (Some(SKY), Some(2.0))
        );

        let image = render(&saved, 0);
        assert_near(
            image.pixel(101.0, 180.0),
            [255, 0, 0],
            30,
            &format!("{layout}: left border"),
        );
        assert_near(
            image.pixel(200.0, 209.0),
            [255, 0, 0],
            30,
            &format!("{layout}: bottom border"),
        );
        assert_near(
            image.pixel(280.0, 200.0),
            [200, 220, 255],
            4,
            &format!("{layout}: fill"),
        );
        assert_eq!(image.pixel(96.0, 180.0), WHITE, "{layout}: outside the box");
        // the first line runs left to right along the top: lots of ink in
        // a wide, short band, and none further down on the left (which is
        // where letters would go if the text had turned with the page)
        let first_line = count(&image, 104, 152, 240, 174, is_dark);
        let below_on_the_left = count(&image, 104, 180, 140, 206, is_dark);
        assert!(
            first_line > 300,
            "{layout}: only {first_line} dark pixels in the first line"
        );
        assert_eq!(below_on_the_left, 0, "{layout}: text is not upright");
    }
}

#[test]
fn text_wraps_by_measured_widths_and_stays_inside_the_box() {
    // with 12 point helvetica, "aaaa" is 26.69 points and a space 3.34,
    // so three words take 86.74 points and four 116.78: more than the
    // 116 points inside a 120 point box with 2 points of padding a side
    let mut doc = load(&pdf_bytes(&[TestPage::letter()], false));
    let info = add_text_box(
        &mut doc,
        0,
        &text_box(
            Rect::new(100.0, 100.0, 120.0, 80.0),
            "aaaa bbbb cccc dddd eeee ffff",
        ),
    )
    .unwrap();
    let reloaded = reload(&mut doc);
    let annotation = annotation_by_name(&reloaded, 0, &info.id);
    let drawing = appearance_text(&reloaded, &annotation);
    assert_eq!(
        drawing.matches(" Tj").count(),
        2,
        "two lines expected:\n{drawing}"
    );
    // the widths go into the font, so every viewer spaces letters the same
    assert!(drawing.contains("/Helv 12 Tf"));

    let image = render(&save(&mut doc), 0);
    let second_line_top = 100.0 + 2.0 + 14.4;
    assert!(
        count(&image, 102, 102, 218, 116, is_dark) > 50,
        "first line drawn"
    );
    assert!(
        count(
            &image,
            102,
            second_line_top as u32,
            218,
            (second_line_top + 13.0) as u32,
            is_dark
        ) > 50,
        "second line drawn"
    );
    assert_eq!(
        count(&image, 220, 95, 300, 185, |pixel| pixel != WHITE),
        0,
        "nothing past the right edge"
    );
}

#[test]
fn long_text_is_clipped_to_the_box() {
    let mut doc = load(&pdf_bytes(&[TestPage::letter()], false));
    let long = "word ".repeat(200);
    add_text_box(
        &mut doc,
        0,
        &text_box(Rect::new(100.0, 100.0, 100.0, 40.0), &long),
    )
    .unwrap();
    let image = render(&save(&mut doc), 0);
    assert!(count(&image, 100, 100, 200, 140, is_dark) > 50);
    assert_eq!(
        count(&image, 95, 141, 205, 300, |pixel| pixel != WHITE),
        0,
        "nothing below the box"
    );
}

#[test]
fn alignment_moves_the_line_across_the_box() {
    let ink_columns = |align: TextAlign| -> (u32, u32) {
        let mut doc = load(&pdf_bytes(&[TestPage::letter()], false));
        let mut params = text_box(Rect::new(100.0, 100.0, 200.0, 30.0), "MM");
        params.align = align;
        params.font_size = 20.0;
        add_text_box(&mut doc, 0, &params).unwrap();
        let image = render(&save(&mut doc), 0);
        let columns: Vec<u32> = (100..300)
            .filter(|x| (100..130).any(|y| is_dark(image.pixel(*x as f32, y as f32))))
            .collect();
        (columns[0], *columns.last().unwrap())
    };
    let (left_start, _) = ink_columns(TextAlign::Left);
    let (center_start, center_end) = ink_columns(TextAlign::Center);
    let (_, right_end) = ink_columns(TextAlign::Right);
    assert!(
        left_start < 106,
        "left aligned starts at the padding: {left_start}"
    );
    assert!(
        (((center_start + center_end) / 2) as i32 - 200).abs() <= 2,
        "centred around 200"
    );
    assert!(
        right_end > 293,
        "right aligned ends at the padding: {right_end}"
    );
}

#[test]
fn characters_the_fonts_cannot_draw_are_named_in_the_error() {
    let mut doc = load(&pdf_bytes(&[TestPage::letter()], false));
    let error = add_text_box(
        &mut doc,
        0,
        &text_box(Rect::new(100.0, 100.0, 100.0, 40.0), "Résumé – 東京 ✓"),
    )
    .unwrap_err();
    assert_eq!(error, AnnotateError::UnsupportedCharacters("東京✓".into()));
    assert!(error.to_string().contains("“東京✓”"), "{error}");
    assert!(page_annotations(&doc, 0).is_empty());
    // latin accents, dashes and curly quotes are fine
    add_text_box(
        &mut doc,
        0,
        &text_box(Rect::new(100.0, 100.0, 100.0, 40.0), "Résumé – “ok” €5"),
    )
    .unwrap();
}

#[test]
fn unusable_sizes_and_boxes_are_refused() {
    let mut doc = load(&pdf_bytes(&[TestPage::letter()], false));
    let mut params = text_box(Rect::new(100.0, 100.0, 100.0, 40.0), "hi");
    params.font_size = 0.0;
    assert!(matches!(
        add_text_box(&mut doc, 0, &params),
        Err(AnnotateError::InvalidValue(_))
    ));
    let empty = text_box(Rect::new(100.0, 100.0, 0.0, 40.0), "hi");
    assert_eq!(
        add_text_box(&mut doc, 0, &empty).unwrap_err(),
        AnnotateError::EmptyArea
    );
    assert!(matches!(
        add_text_box(&mut doc, 3, &text_box(Rect::new(1.0, 1.0, 9.0, 9.0), "x")),
        Err(AnnotateError::UnknownPage(3))
    ));
}

#[test]
fn every_standard_font_draws() {
    for font in [
        StandardFont::Helvetica,
        StandardFont::HelveticaOblique,
        StandardFont::TimesRoman,
        StandardFont::TimesBoldItalic,
        StandardFont::Courier,
        StandardFont::CourierBoldOblique,
    ] {
        let mut doc = load(&pdf_bytes(&[TestPage::letter()], false));
        let mut params = text_box(Rect::new(100.0, 100.0, 300.0, 40.0), "Hamburgefonts");
        params.font = font;
        params.font_size = 24.0;
        add_text_box(&mut doc, 0, &params).unwrap();
        let image = render(&save(&mut doc), 0);
        assert!(
            count(&image, 100, 100, 400, 140, is_dark) > 200,
            "{font:?} drew nothing"
        );
    }
}

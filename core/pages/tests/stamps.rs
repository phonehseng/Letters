//! page numbers, headers, footers and watermarks: where they are drawn
//! (checked in pdfium's pixels), how they are tagged, and taking them off.

mod support;

use letters_pages::{
    add_header_footer, add_image_watermark, add_page_numbers, add_text_watermark, remove_stamps,
    HeaderFooterParams, HorizontalPlace, ImageWatermarkParams, NumberFormat, PageNumberParams,
    PagesError, RemoveStampsParams, Rgb, StampKind, StampLayer, StampPosition, TextWatermarkParams,
    VerticalEdge,
};
use lopdf::{Dictionary, Document, Object};
use support::*;

const BLUE: [u8; 3] = [0, 0, 255];

/// four pages that all show as 300 by 200, each stored differently:
/// plain, turned a quarter, cropped, and with paper that doesn't start at 0,0.
fn differently_stored_pages() -> Document {
    let mut doc = build(&[
        PageSpec::new(300.0, 200.0),
        PageSpec::new(200.0, 300.0).rotated(90),
        PageSpec::new(400.0, 300.0),
        PageSpec::new(300.0, 200.0).origin(1000.0, -500.0),
    ]);
    let cropped = page_ids(&doc)[2];
    doc.get_dictionary_mut(cropped)
        .unwrap()
        .set("CropBox", rect(50.0, 50.0, 350.0, 250.0));
    doc
}

fn big_numbers(position: StampPosition) -> PageNumberParams {
    PageNumberParams {
        position,
        start_number: 8,
        font_size: 40.0,
        margin: 20.0,
        ..PageNumberParams::default()
    }
}

/// checks that ink fills about the box given (left, top, right, bottom).
fn assert_ink_near(shown: &Rendered, expected: (u32, u32, u32, u32), what: &str) {
    let found = shown
        .image
        .ink_bounds()
        .unwrap_or_else(|| panic!("{what}: nothing drawn"));
    let close = |a: u32, b: u32| a.abs_diff(b) <= 3;
    assert!(
        close(found.0, expected.0)
            && close(found.1, expected.1)
            && close(found.2, expected.2)
            && close(found.3, expected.3),
        "{what}: ink at {found:?}, expected about {expected:?}"
    );
}

#[test]
fn page_numbers_land_in_the_same_place_however_the_page_is_stored() {
    let mut doc = differently_stored_pages();
    let position = StampPosition {
        vertical: VerticalEdge::Bottom,
        horizontal: HorizontalPlace::Center,
    };
    add_page_numbers(&mut doc, &big_numbers(position)).unwrap();
    let doc = reload(&mut doc);
    // "8" at 40 points is 22 wide and its top is 29 above the baseline,
    // which sits 20 above the bottom: x 139 to 161, y 151 to 180
    for (index, label) in [(0, "plain"), (1, "turned"), (2, "cropped"), (3, "offset")] {
        let shown = render(&doc, index, &format!("numbers-{label}"));
        assert_eq!((shown.width_pt, shown.height_pt), (300.0, 200.0), "{label}");
        // each page counts up: 8, 9, 10, 11; only the first is a single "8"
        if index == 0 {
            assert_ink_near(&shown, (139, 151, 161, 180), label);
        } else {
            let (left, top, right, bottom) = shown.image.ink_bounds().unwrap();
            let middle = (left + right) / 2;
            assert!(middle.abs_diff(150) <= 3, "{label}: centred at {middle}");
            assert!(
                top.abs_diff(151) <= 3 && bottom.abs_diff(180) <= 3,
                "{label}: rows {top}..{bottom}"
            );
        }
    }
}

#[test]
fn page_numbers_in_the_top_right_corner() {
    let mut doc = differently_stored_pages();
    let position = StampPosition {
        vertical: VerticalEdge::Top,
        horizontal: HorizontalPlace::Right,
    };
    let params = PageNumberParams {
        pages: Some(vec![1]),
        start_number: 4,
        ..big_numbers(position)
    };
    add_page_numbers(&mut doc, &params).unwrap();
    let shown = render(&reload(&mut doc), 1, "numbers-top-right");
    // "4" is 22 wide, ending 20 from the right; its top 20 from the top
    assert_ink_near(&shown, (258, 20, 280, 49), "turned page, top right");
}

#[test]
fn page_number_formats_and_skipping_the_title_page() {
    let formats = [
        (NumberFormat::Number, "(3)"),
        (NumberFormat::PageWord, "(Page 3)"),
        (NumberFormat::OfTotal, "(3 of 5)"),
        (NumberFormat::Roman, "(iii)"),
    ];
    for (format, expected) in formats {
        let mut doc = tagged(5);
        let params = PageNumberParams {
            format,
            skip_first_page: true,
            ..PageNumberParams::default()
        };
        add_page_numbers(&mut doc, &params).unwrap();
        let doc = reload(&mut doc);
        let pages = page_ids(&doc);
        assert!(
            !content_text(&doc, pages[0]).contains("LettersStamp"),
            "the first page is skipped"
        );
        assert!(
            content_text(&doc, pages[2]).contains(expected),
            "{format:?}"
        );
    }
}

#[test]
fn headers_fill_in_their_tokens() {
    let mut doc = tagged(3);
    let params = HeaderFooterParams {
        edge: VerticalEdge::Top,
        left: "{file}".into(),
        center: "Page {page} / {pages}".into(),
        right: "{date}".into(),
        file_name: "Essay (final).pdf".into(),
        date: "11 November 2026".into(),
        ..HeaderFooterParams::default()
    };
    add_header_footer(&mut doc, &params).unwrap();
    let doc = reload(&mut doc);
    let text = content_text(&doc, page_ids(&doc)[1]);
    assert!(text.contains("(Essay \\(final\\).pdf)"), "{text}");
    assert!(text.contains("(Page 2 / 3)"));
    assert!(text.contains("(11 November 2026)"));
    assert!(text.contains("/LettersStamp <</Kind /HeaderFooter>> BDC"));
}

#[test]
fn text_that_the_standard_fonts_cannot_draw_is_refused() {
    let mut doc = tagged(2);
    let before = fingerprint(&doc);
    let params = HeaderFooterParams {
        center: "Report 日本".into(),
        ..HeaderFooterParams::default()
    };
    assert_eq!(
        add_header_footer(&mut doc, &params),
        Err(PagesError::UnsupportedCharacter('日'))
    );
    let watermark = TextWatermarkParams {
        text: "Привет".into(),
        ..TextWatermarkParams::default()
    };
    assert_eq!(
        add_text_watermark(&mut doc, &watermark),
        Err(PagesError::UnsupportedCharacter('П'))
    );
    // western european accents are fine
    let accents = TextWatermarkParams {
        text: "Brouillon – à relire".into(),
        ..TextWatermarkParams::default()
    };
    assert_eq!(fingerprint(&doc), before);
    add_text_watermark(&mut doc, &accents).unwrap();
}

#[test]
fn stamp_settings_are_checked_before_anything_changes() {
    let mut doc = tagged(2);
    let before = fingerprint(&doc);
    let tiny = PageNumberParams {
        font_size: 0.0,
        ..PageNumberParams::default()
    };
    assert!(matches!(
        add_page_numbers(&mut doc, &tiny),
        Err(PagesError::InvalidSetting(_))
    ));
    let out_of_range = PageNumberParams {
        pages: Some(vec![0, 9]),
        ..PageNumberParams::default()
    };
    assert_eq!(
        add_page_numbers(&mut doc, &out_of_range),
        Err(PagesError::NoSuchPage(10))
    );
    let empty = HeaderFooterParams::default();
    assert!(matches!(
        add_header_footer(&mut doc, &empty),
        Err(PagesError::InvalidSetting(_))
    ));
    let opaque = TextWatermarkParams {
        text: "x".into(),
        opacity: 1.5,
        ..TextWatermarkParams::default()
    };
    assert!(matches!(
        add_text_watermark(&mut doc, &opaque),
        Err(PagesError::InvalidSetting(_))
    ));
    let not_a_picture = ImageWatermarkParams {
        image: b"GIF89a".to_vec(),
        ..ImageWatermarkParams::default()
    };
    assert!(matches!(
        add_image_watermark(&mut doc, &not_a_picture),
        Err(PagesError::UnsupportedImage(_))
    ));
    assert_eq!(fingerprint(&doc), before);
}

/// a 200 by 200 page painted solid blue.
fn blue_page() -> PageSpec {
    PageSpec::new(200.0, 200.0).drawing("0 0 1 rg 0 0 200 200 re f\n")
}

fn red_picture(layer: StampLayer, opacity: f32) -> ImageWatermarkParams {
    ImageWatermarkParams {
        image: solid_png(4, 4, [255, 0, 0, 255]),
        opacity,
        scale: 0.5,
        layer,
        pages: None,
    }
}

#[test]
fn a_watermark_in_front_covers_the_page_and_one_behind_is_hidden_by_it() {
    let mut front = build(&[blue_page()]);
    add_image_watermark(&mut front, &red_picture(StampLayer::InFront, 1.0)).unwrap();
    let shown = render(&reload(&mut front), 0, "watermark-front");
    // half the page's size, centred: from 50 to 150
    assert_eq!(shown.image.pixel(100, 100), RED);
    assert_eq!(shown.image.pixel(55, 55), RED);
    assert_eq!(shown.image.pixel(45, 45), BLUE);

    let mut behind = build(&[blue_page()]);
    add_image_watermark(&mut behind, &red_picture(StampLayer::Behind, 1.0)).unwrap();
    let shown = render(&reload(&mut behind), 0, "watermark-behind");
    assert_eq!(shown.image.pixel(100, 100), BLUE);
}

#[test]
fn jpeg_pictures_are_stored_as_they_are_and_drawn() {
    let fixtures = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    for (file, expected) in [("red.jpg", RED), ("black-grey.jpg", [0, 0, 0])] {
        let jpeg = std::fs::read(fixtures.join(file)).unwrap();
        let mut doc = build(&[PageSpec::new(200.0, 200.0)]);
        let params = ImageWatermarkParams {
            image: jpeg.clone(),
            opacity: 1.0,
            scale: 0.5,
            ..ImageWatermarkParams::default()
        };
        add_image_watermark(&mut doc, &params).unwrap();
        let doc = reload(&mut doc);
        // the jpeg bytes go into the file unchanged
        let stored = doc.objects.values().any(|object| {
            object
                .as_stream()
                .is_ok_and(|stream| stream.content == jpeg)
        });
        assert!(stored, "{file} is stored as it is");
        let [red, green, blue] = render(&doc, 0, file).image.pixel(100, 100);
        let close = |a: u8, b: u8| a.abs_diff(b) <= 8;
        assert!(
            close(red, expected[0]) && close(green, expected[1]) && close(blue, expected[2]),
            "{file}: got {red} {green} {blue}"
        );
    }
}

#[test]
fn watermark_opacity_lets_the_page_show_through() {
    let mut doc = build(&[blue_page()]);
    add_image_watermark(&mut doc, &red_picture(StampLayer::InFront, 0.5)).unwrap();
    let shown = render(&reload(&mut doc), 0, "watermark-half");
    let [red, green, blue] = shown.image.pixel(100, 100);
    assert!(
        red.abs_diff(128) <= 3 && green == 0 && blue.abs_diff(128) <= 3,
        "got {red} {green} {blue}"
    );
}

#[test]
fn transparent_parts_of_a_picture_stay_see_through() {
    let mut doc = build(&[blue_page()]);
    let clear = ImageWatermarkParams {
        image: solid_png(4, 4, [255, 0, 0, 0]),
        opacity: 1.0,
        ..red_picture(StampLayer::InFront, 1.0)
    };
    add_image_watermark(&mut doc, &clear).unwrap();
    let doc = reload(&mut doc);
    assert_eq!(
        render(&doc, 0, "watermark-clear").image.pixel(100, 100),
        BLUE
    );
}

#[test]
fn text_watermarks_are_centred_and_turned() {
    let draft = |angle_degrees| TextWatermarkParams {
        text: "DRAFT".into(),
        font_size: 40.0,
        color: Rgb {
            red: 255,
            green: 0,
            blue: 0,
        },
        opacity: 1.0,
        angle_degrees,
        ..TextWatermarkParams::default()
    };
    for (index, angle) in [0.0, 90.0].into_iter().enumerate() {
        let mut doc = build(&[
            PageSpec::new(300.0, 300.0),
            PageSpec::new(300.0, 300.0).rotated(270),
        ]);
        add_text_watermark(&mut doc, &draft(angle)).unwrap();
        let doc = reload(&mut doc);
        for page in 0..2 {
            let shown = render(&doc, page, &format!("draft-{index}-{page}"));
            let (left, top, right, bottom) = shown.image.ink_bounds().unwrap();
            let centre = ((left + right) / 2, (top + bottom) / 2);
            assert!(
                centre.0.abs_diff(150) <= 3 && centre.1.abs_diff(150) <= 3,
                "centre {centre:?}"
            );
            let (wide, tall) = (right - left, bottom - top);
            if angle == 0.0 {
                assert!(wide > tall * 3, "level text is wide: {wide} x {tall}");
            } else {
                assert!(tall > wide * 3, "turned text is tall: {wide} x {tall}");
            }
        }
    }
}

#[test]
fn watermark_opacity_is_stored_as_a_graphics_state() {
    let mut doc = tagged(1);
    let params = TextWatermarkParams {
        text: "Copy".into(),
        opacity: 0.25,
        ..TextWatermarkParams::default()
    };
    add_text_watermark(&mut doc, &params).unwrap();
    let doc = reload(&mut doc);
    let resources = inherited(&doc, page_ids(&doc)[0], b"Resources").unwrap();
    let states = resources
        .as_dict()
        .unwrap()
        .get(b"ExtGState")
        .unwrap()
        .as_dict()
        .unwrap();
    let state = doc
        .get_dictionary(states.get(b"LettersAlpha").unwrap().as_reference().unwrap())
        .unwrap();
    assert_eq!(state.get(b"ca").unwrap().as_float().unwrap(), 0.25);
}

#[test]
fn removing_one_kind_of_stamp_leaves_the_rest_and_the_page_itself() {
    let mut doc = tagged(3);
    let original: Vec<Vec<u8>> = page_ids(&doc)
        .into_iter()
        .map(|page| doc.get_page_content(page))
        .collect();
    add_page_numbers(&mut doc, &PageNumberParams::default()).unwrap();
    let watermark = TextWatermarkParams {
        text: "DRAFT".into(),
        layer: StampLayer::Behind,
        ..TextWatermarkParams::default()
    };
    add_text_watermark(&mut doc, &watermark).unwrap();
    let mut doc = reload(&mut doc);

    let removed = remove_stamps(
        &mut doc,
        &RemoveStampsParams {
            kind: StampKind::PageNumbers,
            pages: None,
        },
    )
    .unwrap();
    assert_eq!(removed, 3);
    let mut doc = reload(&mut doc);
    let text = content_text(&doc, page_ids(&doc)[0]);
    assert!(!text.contains("PageNumbers") && text.contains("Watermark"));

    remove_stamps(
        &mut doc,
        &RemoveStampsParams {
            kind: StampKind::Watermark,
            pages: None,
        },
    )
    .unwrap();
    let doc = reload(&mut doc);
    let now: Vec<Vec<u8>> = page_ids(&doc)
        .into_iter()
        .map(|page| doc.get_page_content(page))
        .collect();
    assert_eq!(
        now, original,
        "with every stamp gone the pages are as they were"
    );
    assert_no_dangling_references(&doc);
}

#[test]
fn removing_stamps_from_a_page_without_any_changes_nothing() {
    let mut doc = tagged(2);
    let before = fingerprint(&doc);
    let removed = remove_stamps(
        &mut doc,
        &RemoveStampsParams {
            kind: StampKind::Watermark,
            pages: None,
        },
    )
    .unwrap();
    assert_eq!(removed, 0);
    assert_eq!(fingerprint(&doc), before);
}

#[test]
fn stamping_one_of_two_pages_that_share_drawing_and_resources_leaves_the_other_alone() {
    let mut doc = build(&[
        PageSpec::new(200.0, 200.0).drawing(&red_square(0.0, 0.0, 20.0)),
        PageSpec::new(200.0, 200.0),
    ]);
    let pages = page_ids(&doc);
    let shared_content = doc.get_page_contents(pages[0])[0];
    let mut fonts = Dictionary::new();
    fonts.set("F1", Object::Reference(doc.add_object(Dictionary::new())));
    let mut resources = Dictionary::new();
    resources.set("Font", Object::Dictionary(fonts));
    let shared_resources = doc.add_object(resources);
    for &page in &pages {
        let dict = doc.get_dictionary_mut(page).unwrap();
        dict.set("Contents", Object::Reference(shared_content));
        dict.set("Resources", Object::Reference(shared_resources));
    }
    let resources_before = doc.get_object(shared_resources).unwrap().clone();
    let content_before = doc.get_object(shared_content).unwrap().clone();

    let params = PageNumberParams {
        pages: Some(vec![0]),
        font_size: 40.0,
        ..PageNumberParams::default()
    };
    add_page_numbers(&mut doc, &params).unwrap();
    let watermark = ImageWatermarkParams {
        pages: Some(vec![0]),
        ..red_picture(StampLayer::InFront, 1.0)
    };
    add_image_watermark(&mut doc, &watermark).unwrap();

    assert_eq!(doc.get_object(shared_resources).unwrap(), &resources_before);
    assert_eq!(doc.get_object(shared_content).unwrap(), &content_before);
    let doc = reload(&mut doc);
    let second = render(&doc, 1, "shared-second");
    assert_eq!(
        second.image.ink_bounds(),
        Some((0, 180, 19, 199)),
        "only the shared square"
    );
    let first = render(&doc, 0, "shared-first");
    assert_eq!(first.image.pixel(100, 100), RED);
}

#[test]
fn a_page_that_inherits_its_resources_gets_its_own_copy_when_stamped() {
    let mut doc = tagged(2);
    let pages = page_ids(&doc);
    let root = doc
        .catalog()
        .unwrap()
        .get(b"Pages")
        .unwrap()
        .as_reference()
        .unwrap();
    // both pages now inherit their font from the root of the page tree
    let resources = doc
        .get_dictionary_mut(pages[0])
        .unwrap()
        .remove(b"Resources")
        .unwrap();
    doc.get_dictionary_mut(pages[1])
        .unwrap()
        .remove(b"Resources");
    doc.get_dictionary_mut(root)
        .unwrap()
        .set("Resources", resources.clone());

    let params = PageNumberParams {
        pages: Some(vec![0]),
        ..PageNumberParams::default()
    };
    add_page_numbers(&mut doc, &params).unwrap();
    let doc = reload(&mut doc);
    let own = inherited(&doc, pages[0], b"Resources").unwrap();
    let fonts = own
        .as_dict()
        .unwrap()
        .get(b"Font")
        .unwrap()
        .as_dict()
        .unwrap();
    assert!(
        fonts.has(b"F1") && fonts.has(b"LettersFont"),
        "keeps the inherited font"
    );
    let shared = inherited(&doc, pages[1], b"Resources").unwrap();
    assert_eq!(shared, resources, "the page tree's resources are unchanged");
    assert!(content_text(&doc, pages[0]).contains("(Page 0)"));
}

#[test]
fn a_stamp_is_not_disturbed_by_drawing_that_forgets_to_restore_its_state() {
    // this page scales everything by 10 and never undoes it
    let careless = PageSpec::new(300.0, 200.0).drawing("10 0 0 10 0 0 cm 0 0 1 rg 0 0 1 1 re f\n");
    let mut doc = build(&[careless]);
    let position = StampPosition {
        vertical: VerticalEdge::Bottom,
        horizontal: HorizontalPlace::Center,
    };
    add_page_numbers(&mut doc, &big_numbers(position)).unwrap();
    let shown = render(&reload(&mut doc), 0, "careless");
    // the number is where it belongs, not ten times further out
    let ink_near_number = shown.image.inked(130, 145, 40, 40);
    assert!(ink_near_number > 50, "found {ink_near_number} inked pixels");
}

#[test]
fn stamps_have_the_kind_tag_the_ui_removes_them_by() {
    let mut doc = tagged(1);
    add_page_numbers(&mut doc, &PageNumberParams::default()).unwrap();
    let doc = reload(&mut doc);
    let page = page_ids(&doc)[0];
    let streams = doc.get_page_contents(page);
    let last = doc
        .get_object(*streams.last().unwrap())
        .unwrap()
        .as_stream()
        .unwrap();
    let text = String::from_utf8(last.content.clone()).unwrap();
    assert!(
        text.trim_start()
            .starts_with("/LettersStamp <</Kind /PageNumbers>> BDC"),
        "{text}"
    );
    assert!(text.trim_end().ends_with("EMC"));
    // and the page's original drawing is wrapped so it can't leak state
    let first = doc.get_object(streams[0]).unwrap().as_stream().unwrap();
    assert_eq!(first.content, b"q\n");
}

//! text boxes and floating pictures through docx and odt, and pasting.

mod support;

use letters_office::model::*;
use letters_office::{read_docx, read_odt, write_docx, write_odt};
use support::fixtures::text_box_document;
use support::*;

fn boxes(document: &Document) -> Vec<TextBox> {
    document
        .blocks
        .iter()
        .filter_map(|b| match b {
            Block::TextBox(t) => Some(t.clone()),
            _ => None,
        })
        .collect()
}

/// the parts of a text box both formats must keep (odt has no rotation
/// and keeps no emu sizes).
fn essentials(text_box: &TextBox) -> String {
    let texts: Vec<String> = text_box
        .blocks
        .iter()
        .filter_map(|b| b.paragraph())
        .map(|p| p.plain_text())
        .collect();
    format!(
        "{} {} {} {} {} {:?} {:?} {:?} {:?} {} {texts:?}",
        text_box.horizontal.offset,
        text_box.vertical.offset,
        text_box.width,
        text_box.height,
        text_box.auto_height,
        text_box.wrap,
        text_box.z_order,
        text_box.margins,
        text_box.fill,
        text_box.border.is_some(),
    )
}
#[test]
fn text_boxes_round_trip_through_docx() {
    let document = text_box_document();
    let saved = write_docx(&document).unwrap();
    let body = part_text(&saved, "word/document.xml");
    assert!(body.contains("<mc:Choice Requires=\"wps\">"));
    assert!(body.contains("<mc:Fallback><w:pict><v:rect"));
    assert_eq!(
        body.matches("<w:txbxContent>").count(),
        4,
        "each box twice: shape and fallback"
    );
    let again = read_docx(&saved).unwrap();
    let (before, after) = (boxes(&document), boxes(&again));
    assert_eq!(after.len(), 2);
    for (a, b) in before.iter().zip(&after) {
        assert_eq!(essentials(a), essentials(b));
        assert_eq!(a.horizontal.relative_to, b.horizontal.relative_to);
        assert_eq!(a.vertical.relative_to, b.vertical.relative_to);
    }
    assert!(after[0].blocks[1].paragraph().unwrap().props.list.is_some());
    assert_eq!(
        after[0].blocks[0].paragraph().unwrap().content[0].marks(),
        &[Mark::Bold]
    );
    // a second save is a fixed point
    let third = read_docx(&write_docx(&again).unwrap()).unwrap();
    assert_eq!(boxes(&third), after);
}

#[test]
fn text_boxes_round_trip_through_odt_and_across() {
    let document = text_box_document();
    let odt = write_odt(&document).unwrap();
    let content = part_text(&odt, "content.xml");
    assert!(content.contains("<draw:text-box"));
    let from_odt = read_odt(&odt).unwrap();
    let (before, after) = (boxes(&document), boxes(&from_odt));
    assert_eq!(after.len(), 2);
    for (a, b) in before.iter().zip(&after) {
        assert_eq!(essentials(a), essentials(b));
    }
    // odt to docx and back keeps them too
    let docx = read_docx(&write_docx(&from_odt).unwrap()).unwrap();
    for (a, b) in after.iter().zip(&boxes(&docx)) {
        assert_eq!(essentials(a), essentials(b));
    }
}

fn picture_document(placement: Placement) -> Document {
    let mut document = Document::new();
    document.media.push(MediaItem {
        id: "dot.png".into(),
        content_type: "image/png".into(),
        source_path: None,
        bytes: png(),
    });
    document.blocks = vec![Block::Paragraph(Paragraph {
        content: vec![
            Inline::text("Picture: "),
            Inline::Image(Image {
                media_id: "dot.png".into(),
                width: 2880,
                height: 1440,
                width_emu: Some(1_828_801),
                height_emu: Some(914_400),
                description: Some("a dot".into()),
                placement,
                z_order: Some(9),
                rotation: Some(15.0),
                crop: Some(Crop {
                    left: 1000,
                    top: 0,
                    right: 2500,
                    bottom: 0,
                }),
                lock_aspect_ratio: Some(true),
                ..Default::default()
            }),
        ],
        ..Default::default()
    })];
    document
}

fn first_image(document: &Document) -> Image {
    document
        .blocks
        .iter()
        .filter_map(Block::paragraph)
        .flat_map(|p| &p.content)
        .find_map(|i| match i {
            Inline::Image(image) => Some(image.clone()),
            _ => None,
        })
        .unwrap()
}

#[test]
fn floating_pictures_keep_full_geometry_exactly() {
    let placement = Placement::Floating {
        horizontal: FloatPosition {
            relative_to: "page".into(),
            offset: 2000,
            align: None,
            offset_emu: Some(1_270_001),
        },
        vertical: FloatPosition::new("paragraph", 300),
        wrap: Wrap::Tight,
    };
    let document = picture_document(placement.clone());
    let saved = write_docx(&document).unwrap();
    let body = part_text(&saved, "word/document.xml");
    assert!(
        body.contains("<wp:posOffset>1270001</wp:posOffset>"),
        "emu-exact offset"
    );
    assert!(body.contains("cx=\"1828801\""), "emu-exact size");
    let image = first_image(&read_docx(&saved).unwrap());
    assert_eq!(
        image.placement,
        Placement::Floating {
            horizontal: FloatPosition {
                offset_emu: Some(1_270_001),
                ..FloatPosition::new("page", 2000)
            },
            vertical: FloatPosition {
                offset_emu: Some(190_500),
                ..FloatPosition::new("paragraph", 300)
            },
            wrap: Wrap::Tight,
        }
    );
    assert_eq!(image.width_emu, Some(1_828_801));
    assert_eq!(image.rotation, Some(15.0));
    assert_eq!(
        image.crop,
        Some(Crop {
            left: 1000,
            top: 0,
            right: 2500,
            bottom: 0
        })
    );
    assert_eq!(image.lock_aspect_ratio, Some(true));
    assert_eq!(image.z_order, Some(9));
    assert_eq!(image.description.as_deref(), Some("a dot"));
    assert_eq!(read_docx(&saved).unwrap().media[0].bytes, png());
}

#[test]
fn toggling_a_picture_through_a_save_is_lossless() {
    let placement = Placement::Floating {
        horizontal: FloatPosition::new("page", 2000),
        vertical: FloatPosition::new("page", 3000),
        wrap: Wrap::Square,
    };
    let mut document = picture_document(placement.clone());
    letters_office::floating::inline_image(&mut document, 0, 1, None).unwrap();
    assert_eq!(first_image(&document).placement, Placement::Inline);
    // the remembered geometry survives the json the editor keeps
    let json = serde_json::to_string(&document).unwrap();
    let mut document: Document = serde_json::from_str(&json).unwrap();
    letters_office::floating::float_image(&mut document, 0, 1, None, Wrap::Square);
    assert_eq!(first_image(&document).placement, placement);
}

#[test]
fn pasting_html_with_file_pictures_uses_the_loader() {
    let loader =
        |source: &str| -> Option<Vec<u8>> { (source == "file:///C:/Temp/clip.png").then(png) };
    let document = letters_office::import::from_html_with_images(
        "<p>Look: <img src=\"file:///C:/Temp/clip.png\" alt=\"clip\"> <img src=\"https://example.org/x.png\"></p>",
        Some(&loader),
    );
    assert_eq!(document.media.len(), 1);
    assert_eq!(document.media[0].bytes, png());
    let image = first_image(&document);
    assert_eq!((image.width, image.height), (15, 15));
    assert_eq!(image.description.as_deref(), Some("clip"));
}

#[test]
fn a_split_paragraph_saves_as_two_text_boxes() {
    let mut document = Document::new();
    document.blocks = vec![Block::text("The first half. The second half.")];
    letters_office::floating::split_paragraph_into_text_boxes(&mut document, 0, 16).unwrap();
    let again = read_docx(&write_docx(&document).unwrap()).unwrap();
    let texts: Vec<String> = boxes(&again)
        .iter()
        .map(|b| b.blocks[0].paragraph().unwrap().plain_text())
        .collect();
    assert_eq!(texts, ["The first half. ", "The second half."]);
}

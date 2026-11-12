//! margins, paper size and page boxes: content moves with one matrix and
//! annotations stay on what they mark.

mod support;

use letters_edit::{
    add_page_space, page_boxes, page_margins, page_objects, resize_page, set_page_boxes,
    set_page_margins, EditConsent, MarginFit, Margins, PageBoxes, PaperFit, Rect, Side,
};
use lopdf::Document;
use support::{load, render, save, PageSetup, PdfBuilder};

/// a heading near the top with a link on it, and a green box near the
/// bottom; a second page that must never change.
fn letter_pdf(setup: PageSetup) -> Vec<u8> {
    let mut pdf = PdfBuilder::new();
    let link = pdf.add("<< /Type /Annot /Subtype /Link /Rect [72 715 150 732] /Border [0 0 0] >>");
    let content = "BT /F1 14 Tf 72 720 Td (Heading) Tj ET 0 0.5 0 rg 72 72 100 50 re f";
    pdf.page_with_extra(
        &setup,
        &[content],
        "/Font << /F1 3 0 R >>",
        &format!("/Annots [{link} 0 R]"),
    );
    pdf.page(
        &setup,
        &["BT /F1 14 Tf 72 720 Td (Second page) Tj ET"],
        "/Font << /F1 3 0 R >>",
    );
    pdf.finish()
}

fn consent() -> EditConsent {
    EditConsent::default()
}

/// the link's rectangle on screen (the page is upright in these tests).
fn link_rect(doc: &Document) -> Rect {
    let page = doc.get_pages()[&1];
    let annots = doc
        .get_dictionary(page)
        .unwrap()
        .get(b"Annots")
        .unwrap()
        .as_array()
        .unwrap()
        .clone();
    let link = doc
        .get_dictionary(annots[0].as_reference().unwrap())
        .unwrap();
    let r: Vec<f32> = link
        .get(b"Rect")
        .unwrap()
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_float().unwrap())
        .collect();
    let height = page_objects(doc, 0).unwrap().height;
    Rect::new(r[0], height - r[3], r[2] - r[0], r[3] - r[1])
}

fn second_page_bytes(doc: &Document) -> String {
    let page = doc.get_pages()[&2];
    let mut ids = vec![page];
    ids.extend(doc.get_page_contents(page));
    ids.iter()
        .map(|id| format!("{:?}", doc.get_object(*id).unwrap()))
        .collect()
}

#[test]
fn margins_are_measured_from_the_content() {
    let doc = load(&letter_pdf(PageSetup::letter()));
    let margins = page_margins(&doc, 0).unwrap();
    let content = margins.content.unwrap();
    assert!((margins.margins.left - 72.0).abs() < 0.01);
    assert!(
        (margins.margins.bottom - 72.0).abs() < 0.01,
        "the green box ends 72 points up"
    );
    assert!(
        margins.margins.top < 72.0 && margins.margins.top > 55.0,
        "{:?}",
        margins.margins
    );
    assert!((content.bottom() - 720.0).abs() < 0.01);
}

#[test]
fn new_margins_scale_the_content_and_keep_the_link_on_its_text() {
    let bytes = letter_pdf(PageSetup::letter());
    let mut doc = load(&bytes);
    let untouched = second_page_bytes(&doc);
    let wanted = Margins {
        top: 36.0,
        right: 36.0,
        bottom: 36.0,
        left: 36.0,
    };
    let result = set_page_margins(&mut doc, 0, &wanted, MarginFit::Scale, &consent()).unwrap();
    let margins = result.margins.margins;
    assert!(
        (margins.left - 36.0).abs() < 0.05 && (margins.top - 36.0).abs() < 0.05,
        "{margins:?}"
    );
    assert!(
        margins.bottom >= 35.9 && margins.right >= 35.9,
        "{margins:?}"
    );
    let heading = &result.objects.text_blocks[0].rect;
    let link = link_rect(&doc);
    assert!(
        link.x <= heading.x + 0.5 && link.right() >= heading.right() - 0.5,
        "{link:?} {heading:?}"
    );
    assert!(
        link.y <= heading.y + 0.5 && link.bottom() >= heading.bottom() - 0.5,
        "{link:?} {heading:?}"
    );
    assert_eq!(
        second_page_bytes(&doc),
        untouched,
        "the other page is untouched"
    );
    let after = render(&save(&mut doc), 0);
    let heading_area = (
        heading.x as u32,
        heading.y as u32,
        heading.right() as u32,
        heading.bottom() as u32,
    );
    assert!(
        after.count(heading_area, support::is_dark) > 20,
        "the heading is drawn where it is now read"
    );
}

#[test]
fn shifting_keeps_the_size() {
    let mut doc = load(&letter_pdf(PageSetup::letter()));
    let before = page_margins(&doc, 0).unwrap().content.unwrap();
    let wanted = Margins {
        top: 100.0,
        right: 0.0,
        bottom: 0.0,
        left: 100.0,
    };
    let result = set_page_margins(&mut doc, 0, &wanted, MarginFit::Shift, &consent()).unwrap();
    let after = result.margins.content.unwrap();
    assert!(
        (after.width - before.width).abs() < 0.01
            && (after.x - 100.0).abs() < 0.01
            && (after.y - 100.0).abs() < 0.01
    );
}

#[test]
fn letter_becomes_a4_with_the_content_fitted() {
    let mut doc = load(&letter_pdf(PageSetup::letter()));
    let before = page_objects(&doc, 0).unwrap();
    let result = resize_page(&mut doc, 0, 595.0, 842.0, PaperFit::Fit, &consent()).unwrap();
    assert_eq!(
        (result.objects.width, result.objects.height),
        (595.0, 842.0)
    );
    assert_eq!(
        page_boxes(&doc, 0).unwrap().media,
        Some([0.0, 0.0, 595.0, 842.0])
    );
    let scale = 595.0 / 612.0;
    let (old, new) = (before.paths[0].rect, result.objects.paths[0].rect);
    assert!(
        (new.width - old.width * scale).abs() < 0.01,
        "{old:?} -> {new:?}"
    );
    let link = link_rect(&doc);
    let heading = result.objects.text_blocks[0].rect;
    assert!(
        link.y <= heading.y + 0.5 && link.bottom() >= heading.bottom() - 0.5,
        "{link:?} {heading:?}"
    );
}

#[test]
fn a_binding_margin_adds_space_on_one_side_only() {
    for (rotation, side) in [(0, Side::Left), (90, Side::Top), (270, Side::Right)] {
        let mut doc = load(&letter_pdf(PageSetup::letter().rotated(rotation)));
        let before = page_objects(&doc, 0).unwrap();
        add_page_space(&mut doc, 0, side, 36.0, &consent()).unwrap();
        let after = page_objects(&doc, 0).unwrap();
        let (old, new) = (before.paths[0].rect, after.paths[0].rect);
        match side {
            Side::Left => assert!(
                (new.x - old.x - 36.0).abs() < 0.01
                    && (after.width - before.width - 36.0).abs() < 0.01
            ),
            Side::Top => assert!(
                (new.y - old.y - 36.0).abs() < 0.01
                    && (after.height - before.height - 36.0).abs() < 0.01,
                "rotation {rotation}"
            ),
            _ => assert!(
                (new.x - old.x).abs() < 0.01 && (after.width - before.width - 36.0).abs() < 0.01,
                "rotation {rotation}"
            ),
        }
    }
}

#[test]
fn boxes_are_set_exactly_and_checked() {
    let mut doc = load(&letter_pdf(PageSetup::letter()));
    let boxes = PageBoxes {
        trim: Some([9.0, 9.0, 603.0, 783.0]),
        bleed: Some([0.0, 0.0, 612.0, 792.0]),
        ..PageBoxes::default()
    };
    set_page_boxes(&mut doc, 0, &boxes, &consent()).unwrap();
    let read = page_boxes(&doc, 0).unwrap();
    assert_eq!(read.trim, Some([9.0, 9.0, 603.0, 783.0]));
    assert_eq!(read.media, Some([0.0, 0.0, 612.0, 792.0]));
    let outside = PageBoxes {
        art: Some([-10.0, 0.0, 100.0, 100.0]),
        ..PageBoxes::default()
    };
    let error = set_page_boxes(&mut doc, 0, &outside, &consent()).unwrap_err();
    assert!(error.to_string().contains("inside the MediaBox"));
    assert_eq!(
        page_boxes(&doc, 0).unwrap().art,
        None,
        "a refused change changes nothing"
    );
}

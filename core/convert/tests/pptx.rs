//! pdf pages to slides: the package is unzipped and its shapes read back,
//! and (with --ignored, where powerpoint is installed) opened in
//! powerpoint itself.

mod support;

use std::path::PathBuf;

use letters_convert::*;
use support::*;

const EMU: f64 = 12700.0;

fn slide_page() -> TestPage {
    let mut page = TestPage::letter();
    page.text(Font::HelveticaBold, 24.0, 72.0, 100.0, "Slide Title");
    page.colored_text(
        Font::TimesItalic,
        12.0,
        72.0,
        140.0,
        "A line in italics",
        [0, 90, 200],
    );
    page.fill_rect(72.0, 200.0, 200.0, 100.0, [30, 120, 220]);
    page.line(72.0, 330.0, 300.0, 330.0, 2.0);
    page.image(
        320.0,
        200.0,
        150.0,
        100.0,
        TestImage::solid(3, 2, [250, 200, 0]),
    );
    page.circle(450.0, 450.0, 40.0, [0, 160, 0]);
    page
}

fn export(pages: &[TestPage], name: &str) -> (PathBuf, PptxReport) {
    let dir = temp_dir(name);
    let pdf = dir.join("slides.pdf");
    write_pdf(&pdf, pages);
    let source = PdfSource::open(&pdf).unwrap();
    let path = dir.join("Slides.pptx");
    let mut renderer = TestRenderer::new();
    let report = pdf_to_pptx(
        &source,
        &mut renderer,
        &PptxExportOptions::default(),
        &path,
        &mut |_| {},
        &CancelFlag::new(),
    )
    .unwrap();
    (path, report)
}

/// a shape's offset and size, in points.
fn frame(node: roxmltree::Node) -> (f64, f64, f64, f64) {
    let xfrm = node.descendants().find(|n| n.has_tag_name("xfrm")).unwrap();
    let off = xfrm.children().find(|n| n.has_tag_name("off")).unwrap();
    let ext = xfrm.children().find(|n| n.has_tag_name("ext")).unwrap();
    let number =
        |n: roxmltree::Node, a: &str| n.attribute(a).unwrap().parse::<f64>().unwrap() / EMU;
    (
        number(off, "x"),
        number(off, "y"),
        number(ext, "cx"),
        number(ext, "cy"),
    )
}

#[test]
fn every_object_is_placed_and_reported() {
    let (path, report) = export(&[slide_page()], "slides");
    assert_eq!((report.slide_width, report.slide_height), (612.0, 792.0));
    let slide = &report.slides[0];
    assert_eq!((slide.text_boxes, slide.pictures, slide.shapes), (2, 1, 2));
    assert!(
        slide.background,
        "the circle has curves, so it is in the background"
    );
    assert_eq!(
        slide.in_background,
        ["One drawn shape (curves, see-through or clipped drawing) is part of the background picture."]
    );

    let presentation = read_zip_entry(&path, "ppt/presentation.xml").unwrap();
    assert!(presentation.contains("<p:sldSz cx=\"7772400\" cy=\"10058400\"/>"));
    let xml = read_zip_entry(&path, "ppt/slides/slide1.xml").unwrap();
    let document = roxmltree::Document::parse(&xml).unwrap();

    // the title: a text box whose baseline is where the pdf's is
    let title = document
        .descendants()
        .find(|n| {
            n.has_tag_name("sp")
                && n.descendants()
                    .any(|t| t.has_tag_name("t") && t.text() == Some("Slide Title"))
        })
        .unwrap();
    let (x, y, _, _) = frame(title);
    assert!((x - 72.0).abs() < 0.5, "{x}");
    assert!((y - (100.0 - 0.9 * 24.0)).abs() < 0.5, "{y}");
    let properties = title.descendants().find(|n| n.has_tag_name("rPr")).unwrap();
    assert_eq!(properties.attribute("sz"), Some("2400"));
    assert_eq!(properties.attribute("b"), Some("1"));
    let typeface = title
        .descendants()
        .find(|n| n.has_tag_name("latin"))
        .unwrap();
    assert_eq!(typeface.attribute("typeface"), Some("Arial"));

    let italic = document
        .descendants()
        .find(|n| {
            n.has_tag_name("sp")
                && n.descendants()
                    .any(|t| t.has_tag_name("t") && t.text() == Some("A line in italics"))
        })
        .unwrap();
    let properties = italic
        .descendants()
        .find(|n| n.has_tag_name("rPr"))
        .unwrap();
    assert_eq!(properties.attribute("i"), Some("1"));
    assert_eq!(properties.attribute("sz"), Some("1200"));
    let color = italic
        .descendants()
        .find(|n| n.has_tag_name("srgbClr"))
        .unwrap();
    assert_eq!(color.attribute("val"), Some("005AC8"));
    assert_eq!(
        italic
            .descendants()
            .find(|n| n.has_tag_name("latin"))
            .unwrap()
            .attribute("typeface"),
        Some("Times New Roman")
    );

    // the blue box is a real rectangle, the line a real line
    let rectangle = document
        .descendants()
        .find(|n| {
            n.has_tag_name("sp")
                && n.descendants().any(|g| g.attribute("prst") == Some("rect"))
                && !n.descendants().any(|t| t.has_tag_name("txBody"))
        })
        .unwrap();
    assert_eq!(frame(rectangle), (72.0, 200.0, 200.0, 100.0));
    assert!(rectangle
        .descendants()
        .any(|c| c.attribute("val") == Some("1E78DC")));
    let line = document
        .descendants()
        .find(|n| n.has_tag_name("cxnSp"))
        .unwrap();
    let (lx, ly, lw, lh) = frame(line);
    assert_eq!((lx, ly, lw, lh), (72.0, 330.0, 228.0, 0.0));

    // the picture at its place, the background under everything
    let pictures: Vec<_> = document
        .descendants()
        .filter(|n| n.has_tag_name("pic"))
        .collect();
    assert_eq!(pictures.len(), 2);
    let background = pictures[0];
    assert_eq!(frame(background), (0.0, 0.0, 612.0, 792.0));
    assert_eq!(frame(pictures[1]), (320.0, 200.0, 150.0, 100.0));
    let names = zip_entry_names(&path);
    assert!(
        names.contains(&"ppt/media/slide1-picture1.png".to_string()),
        "{names:?}"
    );
    assert!(
        names.contains(&"ppt/media/slide1-background.png".to_string()),
        "{names:?}"
    );

    // the background shows the circle but not the text, box or picture
    let files = read_zip(&path);
    let background_png = &files
        .iter()
        .find(|(n, _)| n == "ppt/media/slide1-background.png")
        .unwrap()
        .1;
    let picture = decode_png(background_png);
    let at = |x: f32, y: f32| picture.rgb((x * 200.0 / 72.0) as u32, (y * 200.0 / 72.0) as u32);
    assert_eq!(at(450.0, 450.0), [0, 160, 0]);
    assert_eq!(
        at(150.0, 250.0),
        [255, 255, 255],
        "the box was rebuilt, so it isn't drawn again"
    );
    assert_eq!(at(400.0, 250.0), [255, 255, 255], "so was the picture");
    for x in 72..200 {
        for y in 80..102 {
            assert_eq!(
                at(x as f32, y as f32),
                [255, 255, 255],
                "no text in the background at {x},{y}"
            );
        }
    }
}

#[test]
fn pages_of_only_text_and_boxes_need_no_background() {
    let page = || {
        let mut page = TestPage::letter();
        page.text(Font::Helvetica, 12.0, 72.0, 100.0, "Just text");
        page.stroke_rect(72.0, 120.0, 100.0, 40.0, 1.0);
        page
    };
    let (path, report) = export(&[page(), page()], "no-background");
    assert_eq!(report.slides.len(), 2);
    assert!(!report.slides[0].background);
    assert!(report.slides[0].in_background.is_empty());
    assert!(!zip_entry_names(&path)
        .iter()
        .any(|n| n.contains("background")));
    let content_types = read_zip_entry(&path, "[Content_Types].xml").unwrap();
    assert!(content_types.contains("/ppt/slides/slide2.xml"));
}

#[test]
#[ignore = "needs Microsoft PowerPoint"]
fn powerpoint_opens_the_slides_without_repairs() {
    let mut second = TestPage::letter();
    second.text(Font::Helvetica, 14.0, 72.0, 100.0, "Second slide");
    let (path, _) = export(&[slide_page(), second], "powerpoint");
    let path = path.to_string_lossy().into_owned();
    let found = run_office_check("powerpoint-check.ps1", &[&path], "POWERPNT.EXE", 120);
    assert_eq!(found["slides"], "2");
    assert_eq!(found["width"], "612");
    assert_eq!(found["height"], "792");
    assert_eq!(found["slide1.text"], "Slide Title|A line in italics");
    assert_eq!(found["slide2.text"], "Second slide");
    let title: Vec<f64> = found["box.Slide Title"]
        .split(',')
        .take(3)
        .map(|v| v.parse().unwrap())
        .collect();
    assert!(
        (title[0] - 72.0).abs() < 0.5 && (title[1] - 78.4).abs() < 0.5 && title[2] == 24.0,
        "{title:?}"
    );
}

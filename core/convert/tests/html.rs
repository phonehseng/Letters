//! pdf to html: the exact match pages (positions read back from the
//! output) and the easy to edit structure.

mod support;

use std::fs;
use std::path::{Path, PathBuf};

use letters_convert::*;
use support::*;

fn export(
    pages: &[TestPage],
    options: &HtmlExportOptions,
    name: &str,
) -> (PathBuf, HtmlReport, String) {
    let dir = temp_dir(name);
    let pdf = dir.join("doc.pdf");
    write_pdf(&pdf, pages);
    let source = PdfSource::open(&pdf).unwrap();
    let html_path = dir.join("Notes.html");
    let mut renderer = TestRenderer::new();
    let report = pdf_to_html(
        &source,
        &mut renderer,
        options,
        &html_path,
        &mut |_| {},
        &CancelFlag::new(),
    )
    .unwrap();
    let html = fs::read_to_string(&html_path).unwrap();
    (dir, report, html)
}

/// reads the page as xml: everything written is well formed.
fn parse_html(html: &str) -> roxmltree::Document<'_> {
    let options = roxmltree::ParsingOptions {
        allow_dtd: true,
        ..Default::default()
    };
    roxmltree::Document::parse_with_options(html, options).expect("the page is well formed")
}

/// the value of one css property in a style attribute, in points.
fn points(style: &str, property: &str) -> f32 {
    style
        .split(';')
        .find_map(|part| part.trim().strip_prefix(&format!("{property}:")))
        .unwrap_or_else(|| panic!("no {property} in {style}"))
        .trim_end_matches("pt")
        .parse()
        .unwrap()
}

fn page_with_text_and_a_box() -> TestPage {
    let mut page = TestPage::letter();
    page.text(Font::Helvetica, 12.0, 72.0, 100.0, "Hello world");
    page.colored_text(
        Font::TimesBold,
        18.0,
        300.0,
        200.0,
        "Red title",
        [200, 0, 0],
    );
    page.fill_rect(300.0, 300.0, 100.0, 50.0, [0, 0, 255]);
    page.text(Font::Helvetica, 10.0, 72.0, 420.0, "Visit the site");
    page.link(72.0, 410.0, 70.0, 14.0, "https://example.org/");
    page
}

#[test]
fn exact_match_places_every_run_where_the_pdf_draws_it() {
    let options = HtmlExportOptions {
        self_contained: false,
        ..Default::default()
    };
    let (dir, report, html) = export(&[page_with_text_and_a_box()], &options, "fixed");
    let document = parse_html(&html);

    let page = document
        .descendants()
        .find(|n| n.has_tag_name("section"))
        .unwrap();
    assert_eq!(page.attribute("id"), Some("page-1"));
    let page_style = page.attribute("style").unwrap();
    assert_eq!(
        (points(page_style, "width"), points(page_style, "height")),
        (612.0, 792.0)
    );

    let span = |text: &str| {
        document
            .descendants()
            .find(|n| n.has_tag_name("span") && n.text() == Some(text))
            .unwrap_or_else(|| panic!("no span {text}"))
    };
    let hello = span("Hello world").attribute("style").unwrap();
    assert!((points(hello, "left") - 72.0).abs() < 0.5, "{hello}");
    // the top of a line box is 0.84 of the size above the baseline
    assert!(
        (points(hello, "top") - (100.0 - 0.84 * 12.0)).abs() < 0.5,
        "{hello}"
    );
    assert_eq!(points(hello, "font-size"), 12.0);
    assert!(
        hello.contains("font-family:'Helvetica','Arial',sans-serif"),
        "{hello}"
    );

    let title = span("Red title").attribute("style").unwrap();
    assert!((points(title, "left") - 300.0).abs() < 0.5);
    assert!((points(title, "top") - (200.0 - 0.84 * 18.0)).abs() < 0.5);
    assert!(title.contains("font-weight:bold"));
    assert!(title.contains("color:#C80000"));
    assert!(title.contains("'Times'"));

    let link = span("Visit the site").parent().unwrap();
    assert_eq!(link.attribute("href"), Some("https://example.org/"));

    // the background is a file beside the page, drawn without the text
    let background = document
        .descendants()
        .find(|n| n.has_tag_name("img"))
        .unwrap()
        .attribute("src")
        .unwrap();
    assert_eq!(background, "Notes_files/page-1.png");
    let picture = decode_png(&fs::read(dir.join("Notes_files").join("page-1.png")).unwrap());
    // 150 dpi: 1275 by 1650 pixels
    assert_eq!((picture.width, picture.height), (1275, 1650));
    let at = |x: f32, y: f32| picture.rgb((x * 150.0 / 72.0) as u32, (y * 150.0 / 72.0) as u32);
    assert_eq!(at(350.0, 325.0), [0, 0, 255]);
    let mut text_area_ink = 0;
    for y in 85..102 {
        for x in 72..140 {
            if at(x as f32, y as f32) != [255, 255, 255] {
                text_area_ink += 1;
            }
        }
    }
    assert_eq!(
        text_area_ink, 0,
        "the text must not be in the background too"
    );
    assert_eq!(report.pages_with_background, vec![0]);
    assert_eq!(report.files.len(), 2);
}

#[test]
fn pages_of_only_text_get_no_background_and_one_file() {
    let mut page = TestPage::letter();
    page.text(Font::Helvetica, 12.0, 72.0, 100.0, "Only words here.");
    let (_, report, html) = export(&[page], &HtmlExportOptions::default(), "fixed-plain");
    assert!(!html.contains("<img"));
    assert_eq!(report.files.len(), 1);
    assert!(report.pages_with_background.is_empty());
}

#[test]
fn self_contained_pages_carry_their_pictures() {
    let (_, report, html) = export(
        &[page_with_text_and_a_box()],
        &HtmlExportOptions::default(),
        "fixed-inline",
    );
    assert!(html.contains("src=\"data:image/png;base64,"));
    assert_eq!(report.files.len(), 1);
}

#[test]
fn easy_to_edit_writes_clean_structure() {
    let options = HtmlExportOptions {
        mode: HtmlMode::EasyToEdit,
        self_contained: false,
        ..Default::default()
    };
    let (dir, _, html) = export(&handbook_pages(), &options, "semantic");
    let document = parse_html(&html);
    let texts = |tag: &str| -> Vec<String> {
        document
            .descendants()
            .filter(|n| n.has_tag_name(tag))
            .map(|n| {
                n.descendants()
                    .filter(|d| d.is_text())
                    .filter_map(|d| d.text())
                    .collect::<String>()
            })
            .collect()
    };
    assert_eq!(texts("title"), ["Chemistry Handbook"]);
    assert_eq!(texts("h1"), ["Chemistry Handbook"]);
    assert_eq!(texts("h2"), ["Safety rules"]);
    assert!(
        !html.contains("Course Handbook"),
        "the repeated header is left out"
    );
    assert!(!html.contains("style=\""), "no inline styles");

    // the bullets: three items, the second holding a list of its own
    let ul = document
        .descendants()
        .find(|n| n.has_tag_name("ul"))
        .unwrap();
    let items: Vec<_> = ul.children().filter(|n| n.has_tag_name("li")).collect();
    assert_eq!(items.len(), 3);
    let nested = items[1].children().find(|n| n.has_tag_name("ul")).unwrap();
    assert_eq!(
        nested.children().filter(|n| n.has_tag_name("li")).count(),
        1
    );
    assert_eq!(texts("ol").len(), 1);
    assert_eq!(
        document
            .descendants()
            .filter(|n| n.has_tag_name("ol"))
            .flat_map(|n| n.children())
            .filter(|n| n.has_tag_name("li"))
            .count(),
        2
    );

    // the ruled table: caption, header cells with scope, two body rows
    let table = document
        .descendants()
        .find(|n| n.has_tag_name("table"))
        .unwrap();
    let caption = table
        .children()
        .find(|n| n.has_tag_name("caption"))
        .unwrap();
    assert_eq!(caption.text(), Some("Table 1: Melting points"));
    let headers: Vec<_> = table
        .descendants()
        .filter(|n| n.has_tag_name("th"))
        .collect();
    assert_eq!(headers.len(), 3);
    assert!(headers.iter().all(|h| h.attribute("scope") == Some("col")));
    let body_rows = table
        .descendants()
        .find(|n| n.has_tag_name("tbody"))
        .unwrap()
        .children()
        .filter(|n| n.has_tag_name("tr"))
        .count();
    assert_eq!(body_rows, 2);
    assert_eq!(
        document
            .descendants()
            .filter(|n| n.has_tag_name("table"))
            .count(),
        2
    );

    // the picture: a file beside the page, its caption as alt text
    let img = document
        .descendants()
        .find(|n| n.has_tag_name("img"))
        .unwrap();
    assert_eq!(img.attribute("alt"), Some("Figure 1: A flame test."));
    let src = img.attribute("src").unwrap();
    assert!(Path::new(&dir).join(src).exists(), "{src}");
    assert_eq!(texts("figcaption"), ["Figure 1: A flame test."]);

    let link = document
        .descendants()
        .find(|n| n.has_tag_name("a"))
        .unwrap();
    assert_eq!(link.attribute("href"), Some("https://example.org/guide"));
    let notes = document
        .descendants()
        .find(|n| n.attribute("class") == Some("footnotes"))
        .unwrap();
    assert!(notes
        .descendants()
        .filter_map(|n| n.text())
        .collect::<String>()
        .contains("Measured at normal air pressure."));
}

#[test]
fn a_failed_export_leaves_nothing_behind() {
    let dir = temp_dir("html-cancel");
    let pdf = dir.join("doc.pdf");
    write_pdf(&pdf, &[page_with_text_and_a_box()]);
    let source = PdfSource::open(&pdf).unwrap();
    let cancel = CancelFlag::new();
    cancel.cancel();
    let options = HtmlExportOptions {
        self_contained: false,
        ..Default::default()
    };
    let result = pdf_to_html(
        &source,
        &mut TestRenderer::new(),
        &options,
        &dir.join("Notes.html"),
        &mut |_| {},
        &cancel,
    );
    assert_eq!(result.err(), Some(ConvertError::Cancelled));
    let left: Vec<_> = fs::read_dir(&dir)
        .unwrap()
        .map(|e| e.unwrap().file_name())
        .collect();
    assert_eq!(left, ["doc.pdf"]);
}

//! plain text and html out, plain text and pasted html in.

mod support;

use letters_office::export::{to_html, to_plain_text};
use letters_office::import::{from_html, from_plain_text};
use letters_office::model::*;
use letters_office::read_docx;
use support::fixtures::rich_docx;

#[test]
fn plain_text_export_of_a_rich_document() {
    let document = read_docx(&rich_docx()).unwrap();
    let text = to_plain_text(&document);
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(lines[0], "Introduction");
    assert_eq!(lines[1], "Glowing words[1]");
    assert_eq!(lines[2], "1. first");
    assert_eq!(lines[3], "a) nested");
    assert_eq!(lines[4], "2. second");
    // the deleted word is gone, the inserted one is there
    assert_eq!(lines[5], "The new word");
    assert!(lines.contains(&"Inside a content control"));
    assert!(lines.contains(&"wide\ttall"));
    assert!(lines.contains(&"c\td"));
    assert!(text.contains("Notes\n[1] See the source."));
}

#[test]
fn html_export_of_a_rich_document() {
    let document = read_docx(&rich_docx()).unwrap();
    let html = to_html(&document);
    assert!(html.starts_with("<!DOCTYPE html>"));
    assert!(html.contains("<h1>Introduction</h1>"));
    assert!(html.contains("<strong>Glowing </strong>"));
    assert!(
        html.contains("<ol>\n<li>first<ol>\n<li>nested</li></ol>\n</li>\n<li>second</li></ol>"),
        "{html}"
    );
    assert!(html.contains("<del title=\"Ada\">old</del><ins title=\"Ben\">new</ins>"));
    assert!(html.contains("<a href=\"https://example.org/\">a link</a>"));
    assert!(html.contains("<td colspan=\"2\"><p>wide</p></td><td rowspan=\"2\"><p>tall</p></td>"));
    assert!(html.contains("src=\"data:image/png;base64,iVBORw0KGgo"));
    assert!(html.contains("<li id=\"note-1\">"));
}

#[test]
fn plain_text_import() {
    let document = from_plain_text("First line\r\nName:\tAda\n\nAfter a blank\u{C}New page\n");
    let texts: Vec<String> = document
        .blocks
        .iter()
        .map(|b| {
            b.paragraph()
                .map(|p| p.plain_text())
                .unwrap_or_else(|| "<break>".into())
        })
        .collect();
    assert_eq!(
        texts,
        [
            "First line",
            "Name:\tAda",
            "",
            "After a blank",
            "<break>",
            "New page"
        ]
    );
    assert!(matches!(
        document.blocks[1].paragraph().unwrap().content[1],
        Inline::Tab { .. }
    ));
}

#[test]
fn html_import_keeps_the_safe_subset() {
    let html = "<html><head><title>x</title><style>p{color:red}</style></head><body>\
        <h2>Title &amp; more</h2>\
        <p>Some <b>bold</b>, <i>italic</i> and <a href=\"https://example.org\">a link</a>.\n   More   spaces.</p>\
        <script>alert('no')</script>\
        <p>A <a href=\"javascript:alert(1)\">bad link</a> stays text.</p>\
        <ul><li>one</li><li>two<ol><li>two a</li></ol></li></ul>\
        <table><tr><th colspan=\"2\">head</th></tr><tr><td>a</td><td>b</td></tr></table>\
        <blockquote>Quoted</blockquote><pre>line 1\n  line 2</pre>\
        <p><img src=\"data:image/png;base64,iVBORw0KGgo=\" width=\"10\" height=\"20\" alt=\"tiny\"><img src=\"https://example.org/x.png\"></p>\
        </body></html>";
    let document = from_html(html);
    let Block::Heading(heading) = &document.blocks[0] else {
        panic!("{:?}", document.blocks[0])
    };
    assert_eq!(heading.level, 2);
    assert_eq!(heading.paragraph.plain_text(), "Title & more");
    let paragraph = document.blocks[1].paragraph().unwrap();
    assert_eq!(
        paragraph.plain_text(),
        "Some bold, italic and a link. More spaces."
    );
    assert!(paragraph.content.iter().any(|i| matches!(i, Inline::Text { text, marks } if text == "bold" && marks == &vec![Mark::Bold])));
    assert!(paragraph
        .content
        .iter()
        .any(|i| i.marks().iter().any(
            |m| matches!(m, Mark::Link(t) if t.href.as_deref() == Some("https://example.org"))
        )));
    let bad = document.blocks[2].paragraph().unwrap();
    assert_eq!(bad.plain_text(), "A bad link stays text.");
    assert!(bad
        .content
        .iter()
        .all(|i| !i.marks().iter().any(|m| matches!(m, Mark::Link(_)))));
    let json = serde_json::to_string(&document).unwrap();
    assert!(!json.contains("alert"));
    assert!(!json.contains("color:red"));
    let labels = letters_office::numbering::list_labels(&document);
    let item_labels: Vec<String> = labels.into_iter().flatten().collect();
    assert_eq!(item_labels, ["\u{2022}", "\u{2022}", "\u{25E6}"]);
    let table = document
        .blocks
        .iter()
        .find_map(|b| match b {
            Block::Table(t) => Some(t),
            _ => None,
        })
        .unwrap();
    assert!(table.rows[0].header);
    assert_eq!(table.rows[0].cells[0].col_span, 2);
    assert_eq!(table.rows[1].cells.len(), 2);
    let quote = document
        .blocks
        .iter()
        .filter_map(Block::paragraph)
        .find(|p| p.plain_text() == "Quoted")
        .unwrap();
    assert_eq!(quote.props.indent.left, Some(720));
    let pre = document
        .blocks
        .iter()
        .filter_map(Block::paragraph)
        .find(|p| p.plain_text().starts_with("line 1"))
        .unwrap();
    assert_eq!(pre.plain_text(), "line 1\n  line 2");
    assert_eq!(document.media.len(), 1);
    let image = document
        .blocks
        .iter()
        .filter_map(Block::paragraph)
        .flat_map(|p| p.content.iter())
        .find_map(|i| match i {
            Inline::Image(image) => Some(image),
            _ => None,
        })
        .unwrap();
    assert_eq!((image.width, image.height), (150, 300));
    assert_eq!(image.description.as_deref(), Some("tiny"));
}

#[test]
fn pasted_html_can_be_saved_in_both_formats() {
    let document =
        from_html("<h1>Pasted</h1><p>Text with <em>style</em>.</p><ol><li>a</li><li>b</li></ol>");
    let docx = letters_office::write_docx(&document).unwrap();
    let odt = letters_office::write_odt(&document).unwrap();
    for bytes in [docx, odt] {
        let again = letters_office::open(&bytes).unwrap();
        assert_eq!(letters_office::outline::outline(&again)[0].text, "Pasted");
        let labels: Vec<String> = letters_office::numbering::list_labels(&again)
            .into_iter()
            .flatten()
            .collect();
        assert_eq!(labels, ["1.", "2."]);
    }
}

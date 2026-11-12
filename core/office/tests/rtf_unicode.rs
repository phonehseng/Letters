//! characters written with \u in rtf: emoji (two utf-16 halves each), and
//! the \ucN count of fallback characters that follows them.

use letters_office::model::{Block, Document, Inline, Paragraph};
use letters_office::rtf::{read_rtf, write_rtf};

/// the text of every paragraph, one line each.
fn text_of(rtf: &str) -> String {
    let document = read_rtf(rtf.as_bytes()).unwrap();
    document
        .blocks
        .iter()
        .filter_map(|block| match block {
            Block::Paragraph(paragraph) => Some(paragraph.plain_text()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn an_emoji_written_as_two_halves_is_read_as_one_character() {
    // U+1F600 is the pair D83D DE00, written as signed numbers
    assert_eq!(
        text_of(r"{\rtf1\ansi\uc1 A\u-10179?\u-8704?B}"),
        "A\u{1F600}B"
    );
}

#[test]
fn no_fallback_characters_follow_when_uc_is_0() {
    // macos textedit writes \uc0
    assert_eq!(
        text_of(r"{\rtf1\ansi\uc0 caf\u233 s and na\u239 ve}"),
        "cafés and naïve"
    );
}

#[test]
fn two_fallback_characters_are_skipped_when_uc_is_2() {
    assert_eq!(
        text_of(r"{\rtf1\ansi\uc2 caf\u233 xx and na\u239 yyve}"),
        "café and naïve"
    );
}

#[test]
fn uc_holds_only_for_its_group() {
    assert_eq!(
        text_of(r"{\rtf1\ansi {\uc0 caf\u233 s} na\u239 ?ve}"),
        "cafés naïve"
    );
}

#[test]
fn a_half_without_its_partner_becomes_a_replacement_character() {
    assert_eq!(text_of(r"{\rtf1\ansi A\u-10179?B}"), "A\u{FFFD}B");
}

#[test]
fn emoji_survive_saving_as_rtf_and_reading_it_again() {
    let mut document = Document::new();
    document.blocks = vec![Block::Paragraph(Paragraph {
        content: vec![Inline::text(
            "before \u{1F600} after, caf\u{e9} \u{1F44D}\u{1F3FD}",
        )],
        ..Default::default()
    })];
    let bytes = write_rtf(&document);
    let again = read_rtf(&bytes).unwrap();
    let Block::Paragraph(paragraph) = &again.blocks[0] else {
        panic!("not a paragraph: {:?}", again.blocks[0]);
    };
    assert_eq!(
        paragraph.plain_text(),
        "before \u{1F600} after, caf\u{e9} \u{1F44D}\u{1F3FD}"
    );
}

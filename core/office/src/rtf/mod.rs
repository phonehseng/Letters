//! rich text format (rtf 1.9), written from the model and read back.
//!
//! rtf has no styles that apply by themselves (each paragraph repeats its
//! formatting), so the writer writes the resolved formatting of every
//! paragraph and run. lists are written with their labels as text, which
//! every reader shows, but the numbering is no longer live. comments and
//! tracked deletions are left out; text boxes become frames.

mod read;
mod write;

pub use read::read_rtf;
pub use write::write_rtf;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::*;

    #[test]
    fn writes_and_reads_text_formatting_lists_and_tables() {
        let mut document = Document::new();
        document
            .numbering
            .definitions
            .push(ListDefinition::standard("n", true));
        let list = document.numbering.add_instance("n");
        let mut item = Paragraph::with_text("first item");
        item.props.list = Some(ListRef {
            list_id: list,
            level: 0,
        });
        document.metadata.title = Some("Café {test}".into());
        document.blocks = vec![
            Block::Paragraph(Paragraph {
                content: vec![
                    Inline::text("Plain, "),
                    Inline::styled("bold", vec![Mark::Bold]),
                    Inline::text(" and “ü”."),
                ],
                ..Default::default()
            }),
            Block::Paragraph(item),
            Block::Table(Table {
                rows: vec![TableRow {
                    cells: vec![TableCell::with_text("a"), TableCell::with_text("b")],
                    ..Default::default()
                }],
                ..Default::default()
            }),
            Block::text("after"),
        ];
        let rtf = write_rtf(&document);
        let text = String::from_utf8(rtf.clone()).unwrap();
        assert!(text.starts_with("{\\rtf1"));
        assert!(text.contains("{\\listtext"));
        let back = read_rtf(&rtf).unwrap();
        assert_eq!(back.metadata.title.as_deref(), Some("Café {test}"));
        assert_eq!(
            back.blocks[0].paragraph().unwrap().plain_text(),
            "Plain, bold and “ü”."
        );
        assert!(back.blocks[0].paragraph().unwrap().content.iter().any(|i| matches!(i, Inline::Text { text, marks } if text == "bold" && marks.contains(&Mark::Bold))));
        assert_eq!(
            back.blocks[1].paragraph().unwrap().plain_text(),
            "first item"
        );
        assert!(back.blocks[1].paragraph().unwrap().props.list.is_some());
        let Block::Table(table) = &back.blocks[2] else {
            panic!("{:?}", back.blocks)
        };
        assert_eq!(table.rows[0].cells.len(), 2);
        assert_eq!(back.blocks[3].paragraph().unwrap().plain_text(), "after");
    }

    #[test]
    fn broken_rtf_does_not_panic() {
        for text in [
            "{\\rtf1",
            "{\\rtf1 \\'",
            "{\\rtf1 {{{{",
            "{\\rtf1 \\u-",
            "{\\rtf1}}}}}",
        ] {
            let _ = read_rtf(text.as_bytes());
        }
        assert!(read_rtf(b"hello").is_err());
    }
}

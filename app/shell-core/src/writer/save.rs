//! writing a word processor document to disk, safely.
//!
//! the same rule as the pdf side (see session.rs): the new file is written
//! beside the destination, flushed to the disk and then renamed over it,
//! so a crash or a full disk never leaves half a document behind.

use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use letters_office::model::{Block, Document, FieldKind, Inline};
use letters_office::FileKind;

use crate::session::file_name;

/// writes `document` to `path` in `kind`'s format. the message of an error
/// is written for the person.
pub fn save_document(document: &Document, path: &Path, kind: FileKind) -> Result<(), String> {
    check_can_write(path)?;
    let bytes = letters_office::save(document, kind).map_err(|error| error.to_string())?;
    write_atomically(&bytes, path)
}

/// refuses a read-only file or a folder before anything is written.
fn check_can_write(path: &Path) -> Result<(), String> {
    let name = file_name(path);
    match fs::metadata(path) {
        Ok(meta) if meta.is_dir() => Err(format!("“{name}” is a folder. Choose another name.")),
        Ok(meta) if meta.permissions().readonly() => Err(format!(
            "“{name}” is read-only, so it can’t be replaced. Use Save As to save a copy."
        )),
        _ => Ok(()),
    }
}

/// puts `bytes` at `destination` in one step.
pub fn write_atomically(bytes: &[u8], destination: &Path) -> Result<(), String> {
    let temporary = temporary_path_beside(destination);
    let result = write_and_flush(bytes, &temporary).and_then(|_| fs::rename(&temporary, destination));
    result.map_err(|error| {
        let _ = fs::remove_file(&temporary);
        format!("“{}” couldn’t be saved. ({error})", file_name(destination))
    })
}

fn write_and_flush(bytes: &[u8], path: &Path) -> io::Result<()> {
    let mut file = fs::File::create(path)?;
    file.write_all(bytes)?;
    file.sync_all()
}

fn temporary_path_beside(destination: &Path) -> PathBuf {
    destination.with_file_name(format!(".{}.letters-saving", file_name(destination)))
}

/// what saving as odt would lose, in plain words, or `None` when nothing
/// would be lost. the list follows "what is modelled, preserved or lost"
/// in core/office/README.md.
pub fn odt_losses(document: &Document, read_from: FileKind) -> Option<String> {
    let mut losses = Vec::new();
    let preserved = count_preserved(document);
    if read_from == FileKind::Docx && preserved > 0 {
        losses.push(format!(
            "{preserved} {} Letters keeps as they are (such as charts, text boxes or equations) will keep only their text",
            if preserved == 1 { "item" } else { "items" }
        ));
    }
    if read_from == FileKind::Docx {
        losses.push("tracked formatting changes and moved text are not kept, only tracked insertions and deletions".into());
    }
    if has_date_field(document) {
        losses.push("dates that update themselves lose their format".into());
    }
    if losses.is_empty() {
        return None;
    }
    Some(format!(
        "Saving as OpenDocument (.odt) can’t keep everything: {}. To keep all of it, save as Word (.docx).",
        losses.join("; ")
    ))
}

fn count_preserved(document: &Document) -> usize {
    let mut count = 0;
    visit_blocks(&document.blocks, &mut |block| {
        if matches!(block, Block::Preserved { .. }) {
            count += 1;
        }
        if let Some(paragraph) = block.paragraph() {
            count += paragraph
                .content
                .iter()
                .filter(|inline| matches!(inline, Inline::Preserved { .. } | Inline::PreservedRun { .. }))
                .count();
        }
    });
    count
}

fn has_date_field(document: &Document) -> bool {
    let mut found = false;
    visit_blocks(&document.blocks, &mut |block| {
        if let Some(paragraph) = block.paragraph() {
            found |= paragraph
                .content
                .iter()
                .any(|inline| matches!(inline, Inline::Field(field) if field.kind == FieldKind::Date));
        }
    });
    found
}

/// calls `visit` on every block, tables' cells included.
fn visit_blocks(blocks: &[Block], visit: &mut impl FnMut(&Block)) {
    for block in blocks {
        visit(block);
        if let Block::Table(table) = block {
            for cell in table.rows.iter().flat_map(|row| &row.cells) {
                visit_blocks(&cell.blocks, visit);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use letters_office::model::{Field, Paragraph};

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir()
            .join("letters-writer-tests")
            .join(format!("{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn saving_writes_a_file_that_reads_back_and_leaves_no_temporary_file() {
        let dir = temp_dir("save");
        let path = dir.join("essay.docx");
        let mut document = Document::new();
        document.blocks = vec![Block::text("Hello")];
        save_document(&document, &path, FileKind::Docx).unwrap();
        let read = letters_office::open(&fs::read(&path).unwrap()).unwrap();
        assert_eq!(read.blocks[0].paragraph().unwrap().plain_text(), "Hello");
        assert_eq!(fs::read_dir(&dir).unwrap().count(), 1);
    }

    #[test]
    fn a_read_only_file_is_refused_with_advice() {
        let dir = temp_dir("read-only");
        let path = dir.join("locked.odt");
        fs::write(&path, b"old").unwrap();
        let mut permissions = fs::metadata(&path).unwrap().permissions();
        permissions.set_readonly(true);
        fs::set_permissions(&path, permissions.clone()).unwrap();
        let error = save_document(&Document::new(), &path, FileKind::Odt).unwrap_err();
        assert!(error.contains("Save As"), "{error}");
        assert_eq!(fs::read(&path).unwrap(), b"old");
        #[allow(clippy::permissions_set_readonly_false)]
        permissions.set_readonly(false);
        fs::set_permissions(&path, permissions).unwrap();
    }

    #[test]
    fn odt_losses_are_listed_in_plain_words() {
        let mut document = Document::new();
        assert_eq!(odt_losses(&document, FileKind::Odt), None);
        document.blocks = vec![
            Block::Preserved { xml: "<w:sdt/>".into() },
            Block::Paragraph(Paragraph {
                content: vec![Inline::Field(Field {
                    kind: FieldKind::Date,
                    instruction: "DATE".into(),
                    result: "today".into(),
                    marks: vec![],
                })],
                ..Default::default()
            }),
        ];
        let text = odt_losses(&document, FileKind::Docx).unwrap();
        assert!(text.contains("1 item Letters keeps"), "{text}");
        assert!(text.contains("dates"), "{text}");
        assert!(text.ends_with("save as Word (.docx)."), "{text}");
    }
}

//! tests of every open word processor document (sessions.rs): saving,
//! save as, recovery copies, windows, copies in other formats, conversions.

use super::*;
use letters_office::model::{Block, Document};
use std::fs;

fn folder(name: &str) -> PathBuf {
    let dir = std::env::temp_dir()
        .join("letters-writer-tests")
        .join(format!("sessions-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn docx_with(text: &str, path: &Path) {
    let mut document = Document::new();
    document.blocks = vec![Block::text(text)];
    fs::write(path, letters_office::write_docx(&document).unwrap()).unwrap();
}

fn edited(model: &Value, text: &str) -> Value {
    let mut model = model.clone();
    model["blocks"] =
        serde_json::json!([{ "type": "paragraph", "content": [{ "type": "text", "text": text }] }]);
    model
}

fn read_text(path: &Path) -> String {
    let document = letters_office::open(&fs::read(path).unwrap()).unwrap();
    document.blocks[0].paragraph().unwrap().plain_text()
}

#[test]
fn open_edit_save_and_save_as_odt() {
    let dir = folder("save");
    let path = dir.join("essay.docx");
    docx_with("First draft", &path);
    let sessions = WriterSessions::new(&dir.join("cache"));
    let opened = sessions.open("window-1", &path, false).unwrap();
    assert_eq!(opened.kind, "docx");
    assert!(opened.recovery.is_none());
    sessions
        .save(opened.id, edited(&opened.model, "Second draft"), None)
        .unwrap();
    assert_eq!(read_text(&path), "Second draft");

    let saved = sessions
        .save(
            opened.id,
            edited(&opened.model, "As odt"),
            Some(&dir.join("essay copy")),
        )
        .unwrap();
    assert_eq!(
        saved.kind, "docx",
        "a name without an extension keeps the format"
    );
    let odt = sessions
        .save(
            opened.id,
            edited(&opened.model, "As odt"),
            Some(&dir.join("essay.odt")),
        )
        .unwrap();
    assert_eq!(odt.kind, "odt");
    assert_eq!(
        letters_office::detect(&fs::read(dir.join("essay.odt")).unwrap()).unwrap(),
        FileKind::Odt
    );
    assert_eq!(sessions.path_of(opened.id).unwrap(), dir.join("essay.odt"));
}

#[test]
fn a_crash_leaves_a_recovery_copy_that_the_next_open_offers() {
    let dir = folder("recover");
    let path = dir.join("essay.docx");
    docx_with("Saved words", &path);
    let cache = dir.join("cache");
    let sessions = WriterSessions::new(&cache);
    let opened = sessions.open("window-1", &path, false).unwrap();
    sessions
        .autosave(opened.id, edited(&opened.model, "Unsaved words"))
        .unwrap();
    drop(sessions); // the app ends without saving

    let next_run = WriterSessions::new(&cache);
    let again = next_run.open("window-1", &path, false).unwrap();
    assert!(again.recovery.is_some());
    let recovered = next_run.open("window-1", &path, true).unwrap();
    assert_eq!(
        recovered.model["blocks"][0]["content"][0]["text"],
        "Unsaved words"
    );
    // saving the recovered version makes the copy go away
    next_run
        .save(recovered.id, recovered.model.clone(), None)
        .unwrap();
    assert!(next_run
        .open("window-1", &path, false)
        .unwrap()
        .recovery
        .is_none());
    assert_eq!(read_text(&path), "Unsaved words");
}

#[test]
fn dont_save_discards_the_recovery_copy() {
    let dir = folder("discard");
    let path = dir.join("essay.docx");
    docx_with("Saved", &path);
    let sessions = WriterSessions::new(&dir.join("cache"));
    let opened = sessions.open("w", &path, false).unwrap();
    sessions
        .autosave(opened.id, edited(&opened.model, "Lost"))
        .unwrap();
    sessions.discard_recovery(opened.id).unwrap();
    sessions.close(opened.id);
    assert!(sessions.open("w", &path, false).unwrap().recovery.is_none());
}

#[test]
fn closing_a_window_closes_its_documents_only() {
    let dir = folder("windows");
    let path = dir.join("a.docx");
    docx_with("A", &path);
    let sessions = WriterSessions::new(&dir.join("cache"));
    sessions.open("window-1", &path, false).unwrap();
    let other = sessions.open("window-2", &path, false).unwrap();
    sessions.close_window("window-1");
    assert_eq!(sessions.open_count(), 1);
    assert!(sessions.path_of(other.id).is_some());
}

#[test]
fn saving_over_a_document_another_window_saved_refuses_and_keeps_its_change() {
    let dir = folder("two-windows");
    let path = dir.join("essay.docx");
    docx_with("hello", &path);
    let sessions = WriterSessions::new(&dir.join("cache"));
    let first = sessions.open("window-1", &path, false).unwrap();
    let second = sessions.open("window-2", &path, false).unwrap();
    sessions
        .save(first.id, edited(&first.model, "hello FIRSTWINDOW"), None)
        .unwrap();

    let refused = sessions
        .save(second.id, edited(&second.model, "hello SECONDWINDOW"), None)
        .unwrap_err();
    assert!(refused.contains("Save As"), "{refused}");
    assert_eq!(read_text(&path), "hello FIRSTWINDOW");

    // the first window may go on saving its own file
    sessions
        .save(first.id, edited(&first.model, "hello again"), None)
        .unwrap();
    assert_eq!(read_text(&path), "hello again");
}

#[test]
fn a_file_that_is_not_a_document_gives_a_sentence() {
    let dir = folder("bad");
    let path = dir.join("fake.docx");
    fs::write(&path, b"not a zip").unwrap();
    let error = WriterSessions::new(&dir.join("cache"))
        .open("w", &path, false)
        .unwrap_err();
    assert!(
        error.contains("damaged") || error.contains("isn’t"),
        "{error}"
    );
}

/// writes the ui models the editor's round-trip tests use
/// (app/ui/src/lib/writer/fixtures). run with LETTERS_FIXTURE_DOCX set
/// to a docx made by word and LETTERS_FIXTURE_DIR to the folder.
#[test]
#[ignore = "writes the interface's fixtures from a word-made docx in LETTERS_FIXTURE_DOCX"]
fn write_ui_fixtures() {
    let docx = PathBuf::from(std::env::var("LETTERS_FIXTURE_DOCX").unwrap());
    let out = PathBuf::from(std::env::var("LETTERS_FIXTURE_DIR").unwrap());
    let document = letters_office::open(&fs::read(&docx).unwrap()).unwrap();
    let pretty = |value: &Value| serde_json::to_string_pretty(value).unwrap();
    fs::write(out.join("word-made.json"), pretty(&ui_model(&document))).unwrap();
    let odt = letters_office::write_odt(&document).unwrap();
    let from_odt = letters_office::open(&odt).unwrap();
    fs::write(
        out.join("word-made-as-odt.json"),
        pretty(&ui_model(&from_odt)),
    )
    .unwrap();
}

#[test]
fn copies_in_rich_text_and_markdown_and_rich_text_opens_again() {
    let dir = folder("formats");
    let path = dir.join("essay.docx");
    docx_with("Words to keep", &path);
    let sessions = WriterSessions::new(&dir.join("cache"));
    let opened = sessions.open("w", &path, false).unwrap();
    let rtf = dir.join("essay.rtf");
    sessions
        .export_copy(opened.id, opened.model.clone(), &rtf)
        .unwrap();
    assert!(fs::read(&rtf).unwrap().starts_with(b"{\\rtf"));
    let md = dir.join("essay.md");
    sessions
        .export_copy(opened.id, opened.model.clone(), &md)
        .unwrap();
    assert!(fs::read_to_string(&md).unwrap().contains("Words to keep"));
    assert!(sessions
        .export_copy(opened.id, opened.model.clone(), &dir.join("essay.xyz"))
        .is_err());

    let reopened = sessions.open("w", &rtf, false).unwrap();
    assert_eq!(reopened.converted_from.as_deref(), Some("essay.rtf"));
    assert!(reopened.path.ends_with("essay.docx"));
    assert_eq!(
        reopened.model["blocks"][0]["content"][0]["text"],
        "Words to keep"
    );
}

/// opening and saving without an edit changes nothing the engine can
/// see, text blocks and kept content included
#[test]
fn open_and_save_without_edits_keeps_the_document() {
    use letters_office::model::TextBox;
    let dir = folder("unchanged");
    let path = dir.join("objects.docx");
    let mut document = Document::new();
    document.blocks = vec![
        Block::text("Before"),
        Block::TextBox(TextBox::new(
            2880,
            2880,
            3000,
            1440,
            vec![Block::text("Inside")],
        )),
        Block::Preserved {
            xml:
                "<w:sdt><w:sdtContent><w:p><w:r><w:t>kept</w:t></w:r></w:p></w:sdtContent></w:sdt>"
                    .into(),
        },
        Block::text("After"),
    ];
    fs::write(&path, letters_office::write_docx(&document).unwrap()).unwrap();
    let sessions = WriterSessions::new(&dir.join("cache"));
    let opened = sessions.open("w", &path, false).unwrap();
    let before = letters_office::open(&fs::read(&path).unwrap()).unwrap();
    sessions
        .save(opened.id, opened.model.clone(), None)
        .unwrap();
    let after = letters_office::open(&fs::read(&path).unwrap()).unwrap();
    assert_eq!(after.blocks, before.blocks);
    assert_eq!(after.styles, before.styles);
    assert_eq!(after.sections, before.sections);
}

#[test]
fn formats_are_chosen_by_extension() {
    assert_eq!(
        save_format_for_path(Path::new("a.MD")),
        Some(SaveFormat::Markdown)
    );
    assert_eq!(
        save_format_for_path(Path::new("a.htm")),
        Some(SaveFormat::Html)
    );
    assert_eq!(save_format_for_path(Path::new("a")), None);
}

#[test]
fn names_get_the_right_extension() {
    assert_eq!(
        with_writer_extension(Path::new("a"), FileKind::Odt),
        PathBuf::from("a.odt")
    );
    assert_eq!(
        with_writer_extension(Path::new("a.DOCX"), FileKind::Odt),
        PathBuf::from("a.DOCX")
    );
}

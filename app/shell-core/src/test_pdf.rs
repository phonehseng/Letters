//! small pdf files for the tests of this crate, made with lopdf.

use std::fs;
use std::path::{Path, PathBuf};

use lopdf::{dictionary, Document, Object, Stream};

/// a fresh, empty folder for one test's files.
pub fn temp_dir(test_name: &str) -> PathBuf {
    let dir = std::env::temp_dir()
        .join("letters-shell-tests")
        .join(format!("{test_name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

/// writes a pdf of `page_count` us-letter pages, none of them turned.
pub fn write_pdf(path: &Path, page_count: usize) {
    write_pdf_with_inherited_rotation(path, page_count, None);
}

/// like `write_pdf`, but with /Rotate set on the page tree instead of on
/// each page, which pdf allows: every page then inherits it.
pub fn write_pdf_with_inherited_rotation(path: &Path, page_count: usize, rotate: Option<i64>) {
    let mut document = Document::with_version("1.7");
    let pages_id = document.new_object_id();
    let content = document.add_object(Stream::new(
        dictionary! {},
        b"1 0 0 rg 72 648 72 72 re f".to_vec(),
    ));
    let kids: Vec<Object> = (0..page_count)
        .map(|_| {
            let page = document.add_object(dictionary! {
                "Type" => "Page",
                "Parent" => pages_id,
                "Contents" => content,
            });
            Object::Reference(page)
        })
        .collect();
    let mut pages = dictionary! {
        "Type" => "Pages",
        "Kids" => kids,
        "Count" => page_count as i64,
        "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
    };
    if let Some(rotate) = rotate {
        pages.set("Rotate", rotate);
    }
    document.objects.insert(pages_id, Object::Dictionary(pages));
    let catalog = document.add_object(dictionary! {
        "Type" => "Catalog",
        "Pages" => pages_id,
    });
    document.trailer.set("Root", catalog);
    document.save(path).unwrap();
}

/// the /Rotate a page has in the file, looking up the page tree when the
/// page itself has none. pages are counted from 1, as lopdf does.
pub fn page_rotation(path: &Path, page_number: u32) -> i64 {
    let document = Document::load(path).unwrap();
    let mut node = document.get_pages()[&page_number];
    loop {
        let dictionary = document.get_dictionary(node).unwrap();
        if let Ok(rotate) = dictionary.get(b"Rotate").and_then(Object::as_i64) {
            return rotate;
        }
        match dictionary.get(b"Parent").and_then(Object::as_reference) {
            Ok(parent) => node = parent,
            Err(_) => return 0,
        }
    }
}

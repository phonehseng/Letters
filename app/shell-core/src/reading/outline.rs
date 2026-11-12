//! "Save Bookmarks into the PDF": the reader's bookmarks become real
//! outline entries (what other apps call bookmarks), added after any the
//! file already has. this runs as an edit, so it can be undone and only
//! reaches the file when the document is saved.

use lopdf::{dictionary, Dictionary, Document, Object, ObjectId};

/// one bookmark to write: its name and page (counted from 0).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OutlineBookmark {
    pub title: String,
    pub page: u32,
}

/// adds `bookmarks` to the end of the document's outline.
pub fn add_to_outline(
    document: &mut Document,
    bookmarks: &[OutlineBookmark],
) -> Result<usize, String> {
    let pages = document.get_pages();
    if bookmarks.is_empty() {
        return Ok(0);
    }
    let outlines_id = outlines_root(document)?;
    let mut last = last_child(document, outlines_id);
    let mut added = 0;
    for bookmark in bookmarks {
        let Some(page_id) = pages.get(&(bookmark.page + 1)) else {
            continue;
        };
        let mut item = dictionary! {
            "Title" => text_string(&bookmark.title),
            "Parent" => Object::Reference(outlines_id),
            "Dest" => vec![Object::Reference(*page_id), "Fit".into()],
        };
        if let Some(previous) = last {
            item.set("Prev", Object::Reference(previous));
        }
        let item_id = document.add_object(item);
        match last {
            Some(previous) => set_in(document, previous, "Next", Object::Reference(item_id)),
            None => set_in(document, outlines_id, "First", Object::Reference(item_id)),
        }
        set_in(document, outlines_id, "Last", Object::Reference(item_id));
        last = Some(item_id);
        added += 1;
    }
    let count = count_children(document, outlines_id);
    set_in(document, outlines_id, "Count", Object::Integer(count));
    Ok(added)
}

/// the outline's root, made when the document has none.
fn outlines_root(document: &mut Document) -> Result<ObjectId, String> {
    let existing = document
        .catalog()
        .map_err(|_| "This PDF has no catalog, so bookmarks can\u{2019}t be added.".to_string())?
        .get(b"Outlines")
        .ok()
        .and_then(|o| o.as_reference().ok());
    if let Some(id) = existing {
        if document.get_dictionary(id).is_ok() {
            return Ok(id);
        }
    }
    let id = document.add_object(dictionary! { "Type" => "Outlines", "Count" => 0 });
    document
        .catalog_mut()
        .map_err(|_| "This PDF has no catalog, so bookmarks can\u{2019}t be added.".to_string())?
        .set("Outlines", Object::Reference(id));
    Ok(id)
}

fn last_child(document: &Document, parent: ObjectId) -> Option<ObjectId> {
    let dict = document.get_dictionary(parent).ok()?;
    dict.get(b"Last").ok()?.as_reference().ok()
}

fn count_children(document: &Document, parent: ObjectId) -> i64 {
    let mut count = 0;
    let mut next = document
        .get_dictionary(parent)
        .ok()
        .and_then(|d| d.get(b"First").ok())
        .and_then(|o| o.as_reference().ok());
    // a damaged chain could loop: no outline has a million top-level entries
    while let Some(id) = next {
        count += 1;
        if count > 1_000_000 {
            break;
        }
        next = document
            .get_dictionary(id)
            .ok()
            .and_then(|d: &Dictionary| d.get(b"Next").ok())
            .and_then(|o| o.as_reference().ok());
    }
    count
}

fn set_in(document: &mut Document, id: ObjectId, key: &str, value: Object) {
    if let Ok(dict) = document.get_dictionary_mut(id) {
        dict.set(key, value);
    }
}

/// a title as a pdf text string: utf-16 with a byte order mark, which
/// keeps every language's letters.
fn text_string(text: &str) -> Object {
    let mut bytes = vec![0xFE, 0xFF];
    for unit in text.encode_utf16() {
        bytes.extend_from_slice(&unit.to_be_bytes());
    }
    Object::String(bytes, lopdf::StringFormat::Hexadecimal)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn titles(document: &Document) -> Vec<String> {
        let root = document
            .catalog()
            .unwrap()
            .get(b"Outlines")
            .unwrap()
            .as_reference()
            .unwrap();
        let mut titles = Vec::new();
        let mut next = document
            .get_dictionary(root)
            .unwrap()
            .get(b"First")
            .ok()
            .map(|o| o.as_reference().unwrap());
        while let Some(id) = next {
            let item = document.get_dictionary(id).unwrap();
            if let Ok(Object::String(bytes, _)) = item.get(b"Title") {
                let units: Vec<u16> = bytes[2..]
                    .chunks(2)
                    .map(|p| u16::from_be_bytes([p[0], p[1]]))
                    .collect();
                titles.push(String::from_utf16(&units).unwrap());
            }
            next = item.get(b"Next").ok().map(|o| o.as_reference().unwrap());
        }
        titles
    }

    #[test]
    fn bookmarks_become_outline_entries_after_existing_ones() {
        let folder = crate::test_pdf::temp_dir("outline-bookmarks");
        let path = folder.join("a.pdf");
        crate::test_pdf::write_pdf(&path, 5);
        let mut document = Document::load(&path).unwrap();
        let first = [OutlineBookmark {
            title: "Introduction".into(),
            page: 0,
        }];
        assert_eq!(add_to_outline(&mut document, &first).unwrap(), 1);
        let more = [
            OutlineBookmark {
                title: "Résultats".into(),
                page: 3,
            },
            OutlineBookmark {
                title: "Beyond the end".into(),
                page: 40,
            },
        ];
        assert_eq!(add_to_outline(&mut document, &more).unwrap(), 1);
        // it survives a save and a fresh read
        let saved = folder.join("saved.pdf");
        document.save(&saved).unwrap();
        let reread = Document::load(&saved).unwrap();
        assert_eq!(titles(&reread), vec!["Introduction", "Résultats"]);
    }
}

//! locked files and damaged page lists are refused clearly, and the ui's
//! json settings are read as the shell will send them.

mod support;

use letters_pages::*;
use lopdf::{Document, EncryptionState, EncryptionVersion, Object, Permissions};
use support::*;

/// a document locked with a password, as it comes back from disk.
fn locked() -> Document {
    let mut doc = tagged(3);
    // encryption mixes in the file's id, so the file needs one
    let id = Object::string_literal(b"0123456789abcdef".to_vec());
    doc.trailer.set("ID", Object::Array(vec![id.clone(), id]));
    let version = EncryptionVersion::V2 {
        document: &doc,
        owner_password: "owner",
        user_password: "secret",
        key_length: 128,
        permissions: Permissions::all(),
    };
    let state = EncryptionState::try_from(version).unwrap();
    doc.encrypt(&state).unwrap();
    Document::load_mem(&save(&mut doc)).unwrap()
}

#[test]
fn every_operation_asks_for_a_locked_file_to_be_unlocked_first() {
    let mut doc = locked();
    let open = tagged(2);
    let locked_error = Err(PagesError::Locked);

    assert_eq!(
        reorder(
            &mut doc,
            &ReorderParams {
                new_order: vec![2, 1, 0]
            }
        ),
        locked_error
    );
    assert_eq!(
        move_pages(
            &mut doc,
            &MovePagesParams {
                pages: vec![0],
                before_index: 2
            }
        ),
        locked_error
    );
    assert_eq!(
        rotate(
            &mut doc,
            &RotateParams {
                pages: vec![0],
                quarter_turns: 1
            }
        ),
        locked_error
    );
    assert_eq!(
        delete(&mut doc, &DeleteParams { pages: vec![0] }),
        locked_error
    );
    let blank = InsertBlankParams {
        at_index: 0,
        width_pt: 100.0,
        height_pt: 100.0,
        count: 1,
    };
    assert_eq!(insert_blank(&mut doc, &blank), locked_error);
    let from = InsertFromParams {
        at_index: 0,
        other_pages: vec![0],
    };
    assert_eq!(insert_from(&mut doc, &open, &from), locked_error);
    let mut open_copy = tagged(2);
    assert_eq!(
        insert_from(&mut open_copy, &doc, &from),
        locked_error,
        "a locked source too"
    );
    assert!(matches!(
        extract(&doc, &ExtractParams { pages: vec![0] }),
        Err(PagesError::Locked)
    ));
    let names = MergeParams {
        names: vec!["a".into(), "b".into()],
    };
    assert!(matches!(
        merge(&[open, locked()], &names),
        Err(PagesError::Locked)
    ));
    assert!(matches!(
        split(&doc, &SplitParams::EveryNPages { pages_per_part: 1 }),
        Err(PagesError::Locked)
    ));
    let rect = CropRect {
        left: 0.0,
        top: 0.0,
        width: 50.0,
        height: 50.0,
    };
    assert_eq!(
        crop(
            &mut doc,
            &CropParams {
                pages: vec![0],
                rect
            }
        ),
        locked_error
    );
    assert_eq!(
        uncrop(&mut doc, &UncropParams { pages: vec![0] }),
        locked_error
    );
    assert_eq!(
        add_page_numbers(&mut doc, &PageNumberParams::default()),
        locked_error
    );
    let stamps = RemoveStampsParams {
        kind: StampKind::PageNumbers,
        pages: None,
    };
    assert_eq!(remove_stamps(&mut doc, &stamps).map(|_| ()), locked_error);
}

#[test]
fn the_locked_message_tells_the_person_what_to_do() {
    assert_eq!(
        PagesError::Locked.to_string(),
        "This PDF is locked with a password. Unlock it first, then try again."
    );
    assert_eq!(
        PagesError::NoSuchPage(12).to_string(),
        "Page 12 doesn’t exist in this document."
    );
}

#[test]
fn a_file_without_a_page_list_is_reported_as_damaged() {
    let mut doc = tagged(2);
    doc.catalog_mut().unwrap().remove(b"Pages");
    let result = rotate(
        &mut doc,
        &RotateParams {
            pages: vec![0],
            quarter_turns: 1,
        },
    );
    assert!(matches!(result, Err(PagesError::Damaged(_))));
}

#[test]
fn a_page_tree_that_loops_does_not_hang() {
    let mut doc = tagged(2);
    let root = doc
        .catalog()
        .unwrap()
        .get(b"Pages")
        .unwrap()
        .as_reference()
        .unwrap();
    // the root lists itself as one of its own kids
    let kids = doc
        .get_dictionary_mut(root)
        .unwrap()
        .get_mut(b"Kids")
        .unwrap()
        .as_array_mut()
        .unwrap();
    kids.push(Object::Reference(root));
    reorder(
        &mut doc,
        &ReorderParams {
            new_order: vec![1, 0],
        },
    )
    .unwrap();
    assert_eq!(order(&reload(&mut doc)), [1, 0]);
}

#[test]
fn settings_arrive_from_the_ui_as_camel_case_json() {
    let moved: MovePagesParams =
        serde_json::from_str(r#"{"pages":[2,3],"beforeIndex":0}"#).unwrap();
    assert_eq!(moved.before_index, 0);

    let blank: InsertBlankParams =
        serde_json::from_str(r#"{"atIndex":1,"widthPt":595,"heightPt":842,"count":2}"#).unwrap();
    assert_eq!(blank.height_pt, 842.0);

    // stamp settings can leave out anything that has a sensible default
    let numbers: PageNumberParams = serde_json::from_str(
        r#"{"position":{"vertical":"top","horizontal":"right"},"format":"ofTotal","skipFirstPage":true}"#,
    )
    .unwrap();
    assert_eq!(numbers.position.horizontal, HorizontalPlace::Right);
    assert_eq!(numbers.format, NumberFormat::OfTotal);
    assert_eq!(numbers.start_number, 1);
    assert_eq!(numbers.font_size, 10.0);

    let watermark: TextWatermarkParams = serde_json::from_str(
        r#"{"text":"DRAFT","layer":"behind","color":{"red":200,"green":0,"blue":0}}"#,
    )
    .unwrap();
    assert_eq!(watermark.layer, StampLayer::Behind);
    assert_eq!(watermark.font, StampFont::HelveticaBold);

    let split: SplitParams =
        serde_json::from_str(r#"{"mode":"byRanges","ranges":[{"start":0,"end":4}]}"#).unwrap();
    assert_eq!(
        split,
        SplitParams::ByRanges {
            ranges: vec![PageSpan { start: 0, end: 4 }]
        }
    );
    let by_size: SplitParams =
        serde_json::from_str(r#"{"mode":"byMaxSize","maxBytes":1000000}"#).unwrap();
    assert_eq!(
        by_size,
        SplitParams::ByMaxSize {
            max_bytes: 1_000_000
        }
    );

    let crop: CropParams =
        serde_json::from_str(r#"{"pages":[0],"rect":{"left":1,"top":2,"width":30,"height":40}}"#)
            .unwrap();
    assert_eq!(crop.rect.height, 40.0);

    // and they go back out the same way
    let json = serde_json::to_value(&RemoveStampsParams {
        kind: StampKind::HeaderFooter,
        pages: None,
    })
    .unwrap();
    assert_eq!(json["kind"], "headerFooter");
}

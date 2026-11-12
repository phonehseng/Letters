//! sticky notes, pop-ups, comments on any mark, and reply threads.

mod support;

use letters_annotate::{
    add_note, add_reply, add_text_markup, delete, get, list, update, AnnotateError,
    AnnotationChanges, AnnotationDetails, AnnotationKind, Color, NoteIcon, NoteParams, Point, Rect,
    ReplyParams, TextMarkupKind, TextMarkupParams,
};
use lopdf::{Document, Object};
use support::*;

const ORANGE: Color = Color::new(255, 160, 0);

fn note_at(x: f32, y: f32) -> NoteParams {
    NoteParams {
        position: Point::new(x, y),
        icon: NoteIcon::Comment,
        color: ORANGE,
        contents: "Is this right?".into(),
        author: Some("Ada".into()),
        opacity: 1.0,
        open: false,
    }
}

fn reply(text: &str, author: &str) -> ReplyParams {
    ReplyParams {
        contents: text.into(),
        author: Some(author.into()),
    }
}

fn highlight(doc: &mut Document) -> String {
    let params = TextMarkupParams {
        kind: TextMarkupKind::Highlight,
        rects: vec![Rect::new(100.0, 400.0, 200.0, 14.0)],
        color: Color::HIGHLIGHT_YELLOW,
        opacity: 1.0,
        contents: Some("key point".into()),
        author: None,
    };
    add_text_markup(doc, 0, &params).unwrap().id
}

#[test]
fn a_note_has_an_icon_and_a_popup() {
    let mut doc = load(&pdf_bytes(&[TestPage::letter()], false));
    let info = add_note(&mut doc, 0, &note_at(50.0, 60.0)).unwrap();
    let reloaded = reload(&mut doc);

    let note = annotation_by_name(&reloaded, 0, &info.id);
    assert_eq!(name_of(&note, b"Subtype"), "Text");
    assert_eq!(name_of(&note, b"Name"), "Comment");
    assert_eq!(
        note.get(b"Contents").unwrap().as_str().unwrap(),
        b"Is this right?"
    );
    assert_eq!(note.get(b"T").unwrap().as_str().unwrap(), b"Ada");
    assert!(has_normal_appearance(&reloaded, &note));
    let popup_id = note.get(b"Popup").unwrap().as_reference().unwrap();
    let popup = reloaded.get_dictionary(popup_id).unwrap();
    assert_eq!(name_of(popup, b"Subtype"), "Popup");
    assert_eq!(
        popup.get(b"Parent").unwrap().as_reference().unwrap(),
        reloaded_id_of(&reloaded, &info.id)
    );
    assert_eq!(
        page_annotations(&reloaded, 0).len(),
        2,
        "the note and its pop-up"
    );

    // the pop-up belongs to the note, so only the note is listed
    let listed = list(&reloaded, 0).unwrap();
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].kind, AnnotationKind::Note);
    assert_eq!(listed[0].rect, Rect::new(50.0, 60.0, 20.0, 20.0));
    let AnnotationDetails::Note { icon, popup, .. } = &listed[0].details else {
        panic!("wrong details");
    };
    assert_eq!(*icon, NoteIcon::Comment);
    assert!(popup.unwrap().x >= 70.0, "the pop-up sits beside the icon");
}

/// the object number of the annotation named `id`.
fn reloaded_id_of(doc: &Document, id: &str) -> lopdf::ObjectId {
    let page = doc.get_pages()[&1];
    doc.get_dictionary(page)
        .unwrap()
        .get(b"Annots")
        .unwrap()
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|entry| entry.as_reference().ok())
        .find(|object| {
            doc.get_dictionary(*object)
                .unwrap()
                .get(b"NM")
                .is_ok_and(|name| name.as_str().unwrap() == id.as_bytes())
        })
        .unwrap()
}

#[test]
fn note_icons_are_upright_and_in_place_on_every_page_layout() {
    for (layout, page) in page_variants() {
        let mut doc = load(&pdf_bytes(&[page.with_content("")], false));
        add_note(&mut doc, 0, &note_at(100.0, 100.0)).unwrap();
        let image = render(&save(&mut doc), 0);
        // the speech bubble fills the top of the icon; its bottom right
        // corner is empty (the tail is on the left)
        assert_near(
            image.pixel(115.0, 105.0),
            [255, 160, 0],
            30,
            &format!("{layout}: bubble"),
        );
        assert_eq!(
            image.pixel(116.0, 118.5),
            WHITE,
            "{layout}: no bubble at the bottom right"
        );
        assert_eq!(
            image.pixel(98.0, 110.0),
            WHITE,
            "{layout}: left of the icon"
        );
    }
}

#[test]
fn every_icon_draws_something() {
    for icon in [
        NoteIcon::Comment,
        NoteIcon::Note,
        NoteIcon::Help,
        NoteIcon::Key,
        NoteIcon::Insert,
        NoteIcon::Paragraph,
        NoteIcon::NewParagraph,
    ] {
        let mut doc = load(&pdf_bytes(&[TestPage::letter()], false));
        let mut params = note_at(100.0, 100.0);
        params.icon = icon;
        add_note(&mut doc, 0, &params).unwrap();
        let image = render(&save(&mut doc), 0);
        let coloured = (100..120)
            .flat_map(|y| (100..120).map(move |x| (x, y)))
            .filter(|(x, y)| image.pixel(*x as f32, *y as f32) != WHITE)
            .count();
        assert!(coloured > 40, "{icon:?} drew only {coloured} pixels");
    }
}

#[test]
fn replies_form_a_thread_and_draw_nothing_of_their_own() {
    let mut doc = load(&pdf_bytes(&[TestPage::letter()], false));
    let note = add_note(&mut doc, 0, &note_at(50.0, 60.0)).unwrap();
    let before = render(&save(&mut doc), 0);
    let first = add_reply(&mut doc, 0, &note.id, &reply("Yes, it is.", "Grace")).unwrap();
    let second = add_reply(&mut doc, 0, &first.id, &reply("Thanks!", "Ada")).unwrap();
    let reloaded = reload(&mut doc);

    let listed = list(&reloaded, 0).unwrap();
    assert_eq!(listed.len(), 3);
    let by_id = |id: &str| {
        listed
            .iter()
            .find(|annotation| annotation.id == id)
            .unwrap()
    };
    assert_eq!(by_id(&first.id).reply_to.as_deref(), Some(note.id.as_str()));
    assert_eq!(
        by_id(&second.id).reply_to.as_deref(),
        Some(first.id.as_str())
    );
    assert_eq!(by_id(&first.id).author.as_deref(), Some("Grace"));
    assert_eq!(by_id(&note.id).reply_to, None);

    let reply_dict = annotation_by_name(&reloaded, 0, &first.id);
    assert_eq!(name_of(&reply_dict, b"Subtype"), "Text");
    assert_eq!(name_of(&reply_dict, b"RT"), "R");
    assert!(matches!(reply_dict.get(b"IRT"), Ok(Object::Reference(_))));
    assert!(
        has_normal_appearance(&reloaded, &reply_dict),
        "an (empty) appearance stops viewers drawing their own icon"
    );

    let after = render(&save(&mut doc), 0);
    assert_eq!(
        after.differences(&before, 0),
        0,
        "replies add nothing to the page"
    );
}

#[test]
fn any_mark_can_carry_a_comment_and_a_thread() {
    let mut doc = load(&pdf_bytes(&[TestPage::letter()], false));
    let mark = highlight(&mut doc);
    let changed = update(
        &mut doc,
        0,
        &mark,
        &AnnotationChanges {
            contents: Some(Some("the main argument".into())),
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(changed.contents.as_deref(), Some("the main argument"));
    let answer = add_reply(&mut doc, 0, &mark, &reply("Agreed", "Grace")).unwrap();
    let reloaded = reload(&mut doc);
    assert_eq!(
        get(&reloaded, 0, &answer.id).unwrap().reply_to.as_deref(),
        Some(mark.as_str())
    );
    assert_eq!(
        get(&reloaded, 0, &mark).unwrap().contents.as_deref(),
        Some("the main argument")
    );

    // removing the comment keeps the mark
    let cleared = update(
        &mut doc,
        0,
        &mark,
        &serde_json::from_str(r#"{"contents":null}"#).unwrap(),
    )
    .unwrap();
    assert_eq!(cleared.contents, None);
}

#[test]
fn deleting_a_note_removes_its_popup_and_whole_thread_only() {
    let mut doc = load(&pdf_bytes(&[TestPage::letter()], false));
    let note = add_note(&mut doc, 0, &note_at(50.0, 60.0)).unwrap();
    let first = add_reply(&mut doc, 0, &note.id, &reply("one", "Grace")).unwrap();
    let second = add_reply(&mut doc, 0, &first.id, &reply("two", "Ada")).unwrap();
    let mark = highlight(&mut doc);
    let mark_reply = add_reply(&mut doc, 0, &mark, &reply("three", "Ada")).unwrap();
    let note_object = reloaded_id_of(&doc, &note.id);
    let popup_object = doc
        .get_dictionary(note_object)
        .unwrap()
        .get(b"Popup")
        .unwrap()
        .as_reference()
        .unwrap();

    let removed = delete(&mut doc, 0, &note.id).unwrap();
    assert_eq!(removed.len(), 3);
    for id in [&note.id, &first.id, &second.id] {
        assert!(removed.contains(id), "{id} should be removed");
    }
    assert!(!doc.objects.contains_key(&note_object));
    assert!(
        !doc.objects.contains_key(&popup_object),
        "the pop-up goes too"
    );

    let reloaded = reload(&mut doc);
    let left: Vec<String> = list(&reloaded, 0)
        .unwrap()
        .into_iter()
        .map(|annotation| annotation.id)
        .collect();
    assert_eq!(left, [mark.clone(), mark_reply.id.clone()]);
    // no reference in /Annots points at a missing object
    let page = reloaded.get_pages()[&1];
    for entry in reloaded
        .get_dictionary(page)
        .unwrap()
        .get(b"Annots")
        .unwrap()
        .as_array()
        .unwrap()
    {
        assert!(reloaded
            .get_dictionary(entry.as_reference().unwrap())
            .is_ok());
    }
    assert_eq!(
        delete(&mut doc, 0, &note.id).unwrap_err(),
        AnnotateError::UnknownAnnotation
    );
}

#[test]
fn replying_to_something_missing_is_refused() {
    let mut doc = load(&pdf_bytes(&[TestPage::letter()], false));
    let error = add_reply(&mut doc, 0, "letters-nothing", &reply("hello", "Ada")).unwrap_err();
    assert_eq!(error, AnnotateError::UnknownAnnotation);
}

#[test]
fn ids_stay_the_same_through_save_and_reload() {
    let mut doc = load(&pdf_bytes(&[TestPage::letter(), TestPage::letter()], false));
    let note = add_note(&mut doc, 1, &note_at(50.0, 60.0)).unwrap();
    let mark = highlight(&mut doc);
    let answer = add_reply(&mut doc, 1, &note.id, &reply("ok", "Grace")).unwrap();
    assert!(note.id.starts_with("letters-") && mark.starts_with("letters-"));
    assert_ne!(note.id, answer.id);

    let mut once = reload(&mut doc);
    let mut twice = reload(&mut once);
    let ids = |doc: &Document, page| -> Vec<String> {
        list(doc, page)
            .unwrap()
            .into_iter()
            .map(|annotation| annotation.id)
            .collect()
    };
    assert_eq!(ids(&twice, 1), [note.id.clone(), answer.id.clone()]);
    assert_eq!(ids(&twice, 0), std::slice::from_ref(&mark));

    // and they still work for changes after reloading
    let moved = update(
        &mut twice,
        1,
        &note.id,
        &AnnotationChanges {
            rect: Some(Rect::new(300.0, 300.0, 20.0, 20.0)),
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(moved.id, note.id);
    assert_eq!(moved.rect, Rect::new(300.0, 300.0, 20.0, 20.0));
    let AnnotationDetails::Note { popup, .. } = moved.details else {
        panic!("wrong details");
    };
    assert!(popup.unwrap().x >= 320.0, "the pop-up moved with its note");
}

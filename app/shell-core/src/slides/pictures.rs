//! the pictures of a deck's slides: each slide is drawn by the engine's
//! own renderer into a one-page pdf, which the app's pdf renderer then
//! shows as ordinary tiles (thumbnails, the canvas, the slide show).
//!
//! one pdf per slide means an edit redraws only the slides it changed.
//! each slide keeps the same document id in the pdf renderer for as long
//! as the deck is open; a redrawn slide is written to a new file and the
//! document is pointed at it, with a new revision for the ui's addresses.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use letters_slides::fonts::{FontLibrary, FontSubstitution, SubstitutionReason};
use letters_slides::model::{GraphicKind, Object, Presentation, TextBody, TextParagraph};
use letters_slides::render::render_pdf;
use serde::{Deserialize, Serialize};

use super::view::points;

/// how the shell shows a pdf: the pdf renderer's open, reload and close,
/// behind a trait so these rules can be tested without pdfium.
pub trait PictureViewer {
    /// opens a pdf and returns its document id
    fn open(&self, pdf: &Path) -> Result<u64, String>;
    /// points an open document at a new file, keeping its id
    fn reload(&self, document: u64, pdf: &Path) -> Result<(), String>;
    fn close(&self, document: u64);
}

/// a slide's picture as the ui asks for its tiles.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SlidePicture {
    pub slide_id: String,
    /// the pdf renderer's document: tile://localhost/{document}/0/...
    pub document: u64,
    /// changes whenever the slide is redrawn, so old tiles are not reused
    pub revision: u64,
    /// the page size in points
    pub width: f64,
    pub height: f64,
    /// what drawing it had to approximate, in the engine's words
    pub notes: Vec<String>,
}

/// what to leave out of a picture while it is being typed into, so the
/// typing field's words are the only ones on screen.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum LeftOut {
    Text {
        object: u32,
    },
    Cell {
        object: u32,
        row: usize,
        column: usize,
    },
}

struct Drawn {
    picture: SlidePicture,
    file: PathBuf,
    /// the slide changed since it was drawn
    stale: bool,
}

pub struct SlidePictures {
    folder: PathBuf,
    drawn: HashMap<String, Drawn>,
    /// the one picture of a slide with something left out (see LeftOut)
    editing: Option<Drawn>,
    next_revision: u64,
    fonts: FontLibrary,
}

impl SlidePictures {
    pub fn new(folder: PathBuf) -> SlidePictures {
        SlidePictures {
            folder,
            drawn: HashMap::new(),
            editing: None,
            next_revision: 1,
            fonts: FontLibrary::system(),
        }
    }

    /// the pictures of these slides, drawing those that are new or changed.
    pub fn pictures(
        &mut self,
        presentation: &Presentation,
        slide_ids: &[String],
        viewer: &dyn PictureViewer,
    ) -> Result<Vec<SlidePicture>, String> {
        slide_ids
            .iter()
            .filter_map(|id| presentation.slide_index(id).map(|index| (id, index)))
            .map(|(id, index)| self.picture(presentation, id, index, viewer))
            .collect()
    }

    fn picture(
        &mut self,
        presentation: &Presentation,
        id: &str,
        index: usize,
        viewer: &dyn PictureViewer,
    ) -> Result<SlidePicture, String> {
        if let Some(drawn) = self.drawn.get(id).filter(|drawn| !drawn.stale) {
            return Ok(drawn.picture.clone());
        }
        let previous = self.drawn.remove(id);
        let drawn = self.draw(presentation, id, index, previous, viewer)?;
        let picture = drawn.picture.clone();
        self.drawn.insert(id.to_string(), drawn);
        Ok(picture)
    }

    /// draws a slide into a new file and shows it in the document the
    /// slide had before (or a new one). the old file is removed once the
    /// pdf renderer has let go of it.
    fn draw(
        &mut self,
        presentation: &Presentation,
        id: &str,
        index: usize,
        previous: Option<Drawn>,
        viewer: &dyn PictureViewer,
    ) -> Result<Drawn, String> {
        let (bytes, report) = render_pdf(presentation, Some(&[index]), &mut self.fonts)
            .map_err(|error| error.to_string())?;
        fs::create_dir_all(&self.folder).map_err(|error| error.to_string())?;
        let revision = self.next_revision;
        self.next_revision += 1;
        let file = self.folder.join(format!("slide-{revision}.pdf"));
        fs::write(&file, bytes).map_err(|error| error.to_string())?;
        let document = match &previous {
            Some(old) => {
                viewer.reload(old.picture.document, &file)?;
                let _ = fs::remove_file(&old.file);
                old.picture.document
            }
            None => viewer.open(&file)?,
        };
        let mut notes = report.approximations;
        notes.extend(report.substitutions.iter().map(substitution_sentence));
        Ok(Drawn {
            picture: SlidePicture {
                slide_id: id.to_string(),
                document,
                revision,
                width: points(presentation.slide_size.width),
                height: points(presentation.slide_size.height),
                notes,
            },
            file,
            stale: false,
        })
    }

    /// after an edit: the changed slides (every slide when `all`) are
    /// drawn again when next asked for, and slides that no longer exist
    /// let go of their documents.
    pub fn changed(
        &mut self,
        presentation: &Presentation,
        changed: &[String],
        all: bool,
        viewer: &dyn PictureViewer,
    ) {
        let gone: Vec<String> = self
            .drawn
            .keys()
            .filter(|id| presentation.slide_index(id).is_none())
            .cloned()
            .collect();
        for id in gone {
            if let Some(drawn) = self.drawn.remove(&id) {
                forget(drawn, viewer);
            }
        }
        for (id, drawn) in &mut self.drawn {
            if all || changed.contains(id) {
                drawn.stale = true;
            }
        }
    }

    /// a slide's picture with one object's text (or one table cell's)
    /// left out. the deck is put back exactly as it was before returning.
    pub fn without(
        &mut self,
        presentation: &mut Presentation,
        slide_id: &str,
        left_out: LeftOut,
        viewer: &dyn PictureViewer,
    ) -> Result<SlidePicture, String> {
        let index = presentation
            .slide_index(slide_id)
            .ok_or("This slide is no longer in the presentation.")?;
        let taken = take_text(presentation, index, left_out)
            .ok_or("That text is no longer on the slide.")?;
        let previous = self.editing.take();
        let drawn = self.draw(presentation, slide_id, index, previous, viewer);
        put_text_back(presentation, index, left_out, taken);
        let drawn = drawn?;
        let picture = drawn.picture.clone();
        self.editing = Some(drawn);
        Ok(picture)
    }

    /// lets go of every document and removes the files.
    pub fn close(&mut self, viewer: &dyn PictureViewer) {
        for (_, drawn) in self.drawn.drain() {
            forget(drawn, viewer);
        }
        if let Some(drawn) = self.editing.take() {
            forget(drawn, viewer);
        }
        let _ = fs::remove_dir_all(&self.folder);
    }

    /// the font library the slides are drawn with, for exports.
    pub fn fonts(&mut self) -> &mut FontLibrary {
        &mut self.fonts
    }
}

/// "Aptos isn’t on this computer, so Carlito stands in (same letter widths)."
fn substitution_sentence(substitution: &FontSubstitution) -> String {
    let why = match substitution.reason {
        SubstitutionReason::MetricCompatible => " (same letter widths, so lines break alike)",
        SubstitutionReason::Similar => " (it looks alike, but lines may break differently)",
        _ => "",
    };
    match substitution.reason {
        SubstitutionReason::StyleSynthesised => format!(
            "{} has no bold or italic style here, so it is slanted or thickened.",
            substitution.requested
        ),
        SubstitutionReason::MissingCharacters => format!(
            "Some characters {} can’t draw come from {}.",
            substitution.requested, substitution.used
        ),
        _ => format!(
            "{} isn’t on this computer, so {} stands in{why}.",
            substitution.requested, substitution.used
        ),
    }
}

fn forget(drawn: Drawn, viewer: &dyn PictureViewer) {
    viewer.close(drawn.picture.document);
    let _ = fs::remove_file(drawn.file);
}

/// the text body that is left out, swapped for an empty one with the same
/// box settings (so nothing else on the slide moves).
fn take_text(presentation: &mut Presentation, index: usize, left_out: LeftOut) -> Option<TextBody> {
    let slide = presentation.slides.get_mut(index)?;
    let body = text_slot(&mut slide.objects, left_out)?;
    let mut empty = TextBody {
        body: body.body.clone(),
        list_style: body.list_style.clone(),
        paragraphs: vec![TextParagraph::default()],
    };
    std::mem::swap(body, &mut empty);
    Some(empty)
}

fn put_text_back(
    presentation: &mut Presentation,
    index: usize,
    left_out: LeftOut,
    mut taken: TextBody,
) {
    if let Some(body) = presentation
        .slides
        .get_mut(index)
        .and_then(|slide| text_slot(&mut slide.objects, left_out))
    {
        std::mem::swap(body, &mut taken);
    }
}

fn text_slot(objects: &mut [Object], left_out: LeftOut) -> Option<&mut TextBody> {
    match left_out {
        LeftOut::Text { object } => objects.iter_mut().find(|o| o.id() == object)?.text_mut(),
        LeftOut::Cell {
            object,
            row,
            column,
        } => {
            let Some(Object::Frame(frame)) = objects.iter_mut().find(|o| o.id() == object) else {
                return None;
            };
            let GraphicKind::Table(table) = &mut frame.kind else {
                return None;
            };
            Some(&mut table.rows.get_mut(row)?.cells.get_mut(column)?.text)
        }
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use std::cell::RefCell;

    use super::*;
    use crate::slides::deck::SlideDeck;
    use crate::slides::test_decks::{deck_path, scratch_folder};

    /// a pretend pdf renderer that records what it was asked.
    #[derive(Default)]
    pub struct FakeViewer {
        pub opened: RefCell<Vec<PathBuf>>,
        pub reloaded: RefCell<Vec<(u64, PathBuf)>>,
        pub closed: RefCell<Vec<u64>>,
    }

    impl PictureViewer for FakeViewer {
        fn open(&self, pdf: &Path) -> Result<u64, String> {
            assert!(pdf.is_file(), "the pdf is written before it is opened");
            self.opened.borrow_mut().push(pdf.to_path_buf());
            Ok(self.opened.borrow().len() as u64)
        }

        fn reload(&self, document: u64, pdf: &Path) -> Result<(), String> {
            self.reloaded
                .borrow_mut()
                .push((document, pdf.to_path_buf()));
            Ok(())
        }

        fn close(&self, document: u64) {
            self.closed.borrow_mut().push(document);
        }
    }

    fn ids(presentation: &Presentation) -> Vec<String> {
        presentation.slides.iter().map(|s| s.id.clone()).collect()
    }

    #[test]
    fn each_slide_is_drawn_once_until_it_changes() {
        let deck = SlideDeck::open(&deck_path("pp_title_bullets.pptx")).unwrap();
        let viewer = FakeViewer::default();
        let mut pictures = SlidePictures::new(scratch_folder("pictures-once"));
        let first = pictures
            .pictures(&deck.presentation, &ids(&deck.presentation), &viewer)
            .unwrap();
        assert_eq!(first.len(), 2);
        assert_eq!(viewer.opened.borrow().len(), 2);
        let again = pictures
            .pictures(&deck.presentation, &ids(&deck.presentation), &viewer)
            .unwrap();
        assert_eq!(first, again, "nothing is drawn twice");
        let changed = vec![first[1].slide_id.clone()];
        pictures.changed(&deck.presentation, &changed, false, &viewer);
        let after = pictures
            .pictures(&deck.presentation, &ids(&deck.presentation), &viewer)
            .unwrap();
        assert_eq!(after[0], first[0]);
        assert_eq!(
            after[1].document, first[1].document,
            "a redrawn slide keeps its document"
        );
        assert!(after[1].revision > first[1].revision);
        assert_eq!(viewer.reloaded.borrow().len(), 1);
        assert!((after[0].width - 960.0).abs() < 0.01);
    }

    #[test]
    fn deleted_slides_let_go_of_their_documents_and_closing_removes_the_files() {
        let mut deck = SlideDeck::open(&deck_path("pp_title_bullets.pptx")).unwrap();
        let viewer = FakeViewer::default();
        let folder = scratch_folder("pictures-gone");
        let mut pictures = SlidePictures::new(folder.clone());
        pictures
            .pictures(&deck.presentation, &ids(&deck.presentation), &viewer)
            .unwrap();
        let change = deck
            .apply(&letters_slides::edit::Operation::DeleteSlide { index: 0 })
            .unwrap();
        pictures.changed(
            &deck.presentation,
            &change.changed_slides,
            change.all_slides,
            &viewer,
        );
        assert_eq!(viewer.closed.borrow().len(), 1);
        pictures.close(&viewer);
        assert_eq!(viewer.closed.borrow().len(), 2);
        assert!(!folder.exists());
    }

    #[test]
    fn leaving_text_out_puts_the_deck_back_exactly() {
        let mut deck = SlideDeck::open(&deck_path("pp_title_bullets.pptx")).unwrap();
        let before = serde_json::to_string(&deck.presentation.slides).unwrap();
        let slide = deck.presentation.slides[0].clone();
        let object = slide
            .objects
            .iter()
            .find(|o| o.text().is_some())
            .unwrap()
            .id();
        let viewer = FakeViewer::default();
        let mut pictures = SlidePictures::new(scratch_folder("pictures-left-out"));
        let picture = pictures
            .without(
                &mut deck.presentation,
                &slide.id,
                LeftOut::Text { object },
                &viewer,
            )
            .unwrap();
        assert_eq!(picture.slide_id, slide.id);
        assert_eq!(
            serde_json::to_string(&deck.presentation.slides).unwrap(),
            before
        );
    }

    #[test]
    fn a_table_cell_can_be_left_out() {
        let mut deck = SlideDeck::open(&deck_path("pp_table.pptx")).unwrap();
        let slide = deck.presentation.slides[0].clone();
        let table = slide
            .objects
            .iter()
            .find(|o| matches!(o, Object::Frame(_)))
            .unwrap()
            .id();
        let viewer = FakeViewer::default();
        let mut pictures = SlidePictures::new(scratch_folder("pictures-cell"));
        let left_out = LeftOut::Cell {
            object: table,
            row: 0,
            column: 0,
        };
        assert!(pictures
            .without(&mut deck.presentation, &slide.id, left_out, &viewer)
            .is_ok());
        let missing = LeftOut::Cell {
            object: table,
            row: 99,
            column: 0,
        };
        assert!(pictures
            .without(&mut deck.presentation, &slide.id, missing, &viewer)
            .is_err());
    }
}

//! which package part each slide, layout, master, theme and notes page is
//! written to: the part it was read from when there is one, so names and
//! relationship ids stay as they were, otherwise the next free name.

use std::collections::HashMap;

use super::package::OutPackage;
use crate::model::{Presentation, Source, SourceFormat};

pub struct PartNames {
    /// one per slide, in deck order
    pub slides: Vec<String>,
    /// one per slide: the notes part, when the slide has notes
    pub notes: Vec<Option<String>>,
    pub layouts: HashMap<String, String>,
    pub masters: HashMap<String, String>,
    pub themes: HashMap<String, String>,
    /// for each master id: its own theme part. powerpoint wants one theme
    /// part per master, so a theme two masters share is written twice
    pub master_themes: HashMap<String, String>,
}

/// the part a pptx source was read from.
fn pptx_part(source: Option<&Source>) -> Option<&str> {
    source
        .filter(|source| source.format == SourceFormat::Pptx)
        .and_then(|source| source.part.as_deref())
}

/// hands out part names: a part keeps its original name unless another
/// object already took it, otherwise it gets the lowest free number.
struct Namer<'a> {
    package: &'a OutPackage,
    taken: Vec<String>,
}

impl Namer<'_> {
    fn name(&mut self, original: Option<&str>, prefix: &str) -> String {
        let free =
            |part: &str, taken: &[String]| !taken.iter().any(|t| t.eq_ignore_ascii_case(part));
        let part = match original {
            Some(part) if free(part, &self.taken) => part.to_string(),
            _ => self.package.numbered_name(prefix, &self.taken),
        };
        self.taken.push(part.clone());
        part
    }
}

impl PartNames {
    pub fn assign(presentation: &Presentation, package: &OutPackage) -> PartNames {
        let mut namer = Namer {
            package,
            taken: Vec::new(),
        };
        let themes = theme_names(presentation, &mut namer);
        let (masters, master_themes) = master_names(presentation, &themes, &mut namer);
        let layouts = presentation
            .layouts
            .iter()
            .map(|layout| {
                let part = namer.name(
                    pptx_part(layout.source.as_ref()),
                    "ppt/slideLayouts/slideLayout",
                );
                (layout.id.clone(), part)
            })
            .collect();
        let (slides, notes) = slide_names(presentation, &mut namer);
        PartNames {
            slides,
            notes,
            layouts,
            masters,
            themes,
            master_themes,
        }
    }
}

fn theme_names(presentation: &Presentation, namer: &mut Namer<'_>) -> HashMap<String, String> {
    let mut themes = HashMap::new();
    for theme in &presentation.themes {
        // a pptx theme's id is its part name
        let original = theme
            .source
            .as_ref()
            .filter(|s| s.format == SourceFormat::Pptx)
            .map(|_| theme.id.as_str());
        themes.insert(theme.id.clone(), namer.name(original, "ppt/theme/theme"));
    }
    themes
}

type MasterNames = (HashMap<String, String>, HashMap<String, String>);

/// each master's part and its own theme part (a theme two masters share
/// is copied for the second).
fn master_names(
    presentation: &Presentation,
    themes: &HashMap<String, String>,
    namer: &mut Namer<'_>,
) -> MasterNames {
    let mut masters = HashMap::new();
    let mut master_themes = HashMap::new();
    let mut used_themes: Vec<String> = Vec::new();
    for master in &presentation.masters {
        let part = namer.name(
            pptx_part(master.source.as_ref()),
            "ppt/slideMasters/slideMaster",
        );
        masters.insert(master.id.clone(), part);
        let Some(theme_part) = master
            .theme_id
            .as_ref()
            .and_then(|id| themes.get(id))
            .cloned()
        else {
            continue;
        };
        let own = if used_themes.contains(&theme_part) {
            namer.name(None, "ppt/theme/theme")
        } else {
            theme_part.clone()
        };
        used_themes.push(theme_part);
        master_themes.insert(master.id.clone(), own);
    }
    (masters, master_themes)
}

fn slide_names(
    presentation: &Presentation,
    namer: &mut Namer<'_>,
) -> (Vec<String>, Vec<Option<String>>) {
    let mut slides = Vec::new();
    let mut notes = Vec::new();
    for slide in &presentation.slides {
        let original = presentation.carry.part_name_of(&slide.id);
        let slide_part = namer.name(original, "ppt/slides/slide");
        // a slide keeps its notes part only when it keeps its own part
        let same_part = original == Some(slide_part.as_str());
        let notes_original = slide
            .notes
            .as_ref()
            .and_then(|n| pptx_part(n.source.as_ref()))
            .filter(|_| same_part);
        let notes_part = slide
            .notes
            .as_ref()
            .map(|_| namer.name(notes_original, "ppt/notesSlides/notesSlide"));
        slides.push(slide_part);
        notes.push(notes_part);
    }
    (slides, notes)
}

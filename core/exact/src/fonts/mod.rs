//! fonts: rebuild every pdf face as clean truetype, then one grid font per
//! face and size, and embed them the way word expects (recipe 3 and 4).

pub mod embed;
pub mod grid;
pub mod outline;
pub mod permissions;
pub mod rebuild;
pub mod source;
pub mod standins;
pub mod truetype;
pub mod type1;
pub mod type3;

use std::collections::{BTreeSet, HashMap};

use serde::Serialize;

use crate::pdf::Glyph;
use grid::{make_grid_font, GridFont};
use permissions::may_embed;
use rebuild::{face_key, rebuild_face, rebuild_substitute, unique_family, RebuiltFace};

/// what happened to one pdf face.
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FontReport {
    /// the pdf's name for the face
    pub pdf_font: String,
    pub family: String,
    /// how many pdf fonts (subsets) were merged into this face
    pub subsets: usize,
    pub program: String,
    pub embedded_in_pdf: bool,
    /// set when a stand-in was used: which, and how close it is
    pub substitute: Option<String>,
    /// why the embedded program was not used, when it wasn't
    pub not_used_because: Option<String>,
    pub fs_type: Option<u16>,
    pub permission: String,
    pub cubic_source: bool,
    pub outline_error_em: f64,
    /// glyphs written as private use chars: their text in the docx is not the real text
    pub private_use_glyphs: usize,
    pub problems: Vec<String>,
}

/// every rebuilt face of a document and its grid fonts.
pub struct FontPlan {
    faces: HashMap<String, RebuiltFace>,
    grid_fonts: HashMap<(String, i64), GridFont>,
    /// grid font keys in the order they were made, so the docx is stable
    grid_order: Vec<(String, i64)>,
    pub reports: Vec<FontReport>,
}

/// pdf sizes are grouped to 0.01 pt (recipe 4).
fn size_key(size: f64) -> i64 {
    (size * 100.0).round_ties_even() as i64
}

impl FontPlan {
    /// rebuilds the faces of every font the glyphs use. `document_tag`
    /// makes family names unique to this document.
    pub fn new(glyphs: &[Glyph], document_tag: u64) -> Self {
        let mut face_order: Vec<String> = Vec::new();
        let mut face_fonts: HashMap<String, Vec<&Glyph>> = HashMap::new();
        let mut used: HashMap<String, BTreeSet<u32>> = HashMap::new();
        for glyph in glyphs {
            let key = face_key(&glyph.font);
            let fonts = face_fonts.entry(key.clone()).or_insert_with(|| {
                face_order.push(key.clone());
                Vec::new()
            });
            if !fonts.iter().any(|g| g.font.key == glyph.font.key) {
                fonts.push(glyph);
            }
            used.entry(glyph.font.key.clone())
                .or_default()
                .insert(glyph.code);
        }
        let mut faces = HashMap::new();
        let mut reports = Vec::new();
        for key in face_order {
            let fonts: Vec<&crate::pdf::fonts::PdfFont> =
                face_fonts[&key].iter().map(|g| g.font.as_ref()).collect();
            let family = unique_family(&key, document_tag);
            let (face, report) = build_face(&fonts, &used, &family);
            reports.push(report);
            faces.insert(key, face);
        }
        Self {
            faces,
            grid_fonts: HashMap::new(),
            grid_order: Vec::new(),
            reports,
        }
    }

    fn face(&self, glyph: &Glyph) -> &RebuiltFace {
        &self.faces[&face_key(&glyph.font)]
    }

    /// the grid font for a glyph's face and size, made the first time it is asked for.
    pub fn grid_font(&mut self, glyph: &Glyph) -> Result<&GridFont, String> {
        let face_key = face_key(&glyph.font);
        let size = size_key(glyph.size);
        let key = (face_key.clone(), size);
        if !self.grid_fonts.contains_key(&key) {
            let grid = make_grid_font(&self.faces[&face_key], size as f64 / 100.0)?;
            self.grid_fonts.insert(key.clone(), grid);
            self.grid_order.push(key.clone());
        }
        Ok(&self.grid_fonts[&key])
    }

    /// the char a glyph is written as in the docx (empty when it has none).
    pub fn char_of(&self, glyph: &Glyph) -> String {
        let face = self.face(glyph);
        match face.char_for.get(&(glyph.font.key.clone(), glyph.code)) {
            Some(char) => char.clone(),
            None if glyph.text == " " => " ".into(),
            None => String::new(),
        }
    }

    /// every grid font made, in the order they were first used.
    pub fn grid_fonts(&self) -> Vec<&GridFont> {
        self.grid_order
            .iter()
            .map(|key| &self.grid_fonts[key])
            .collect()
    }
}

/// one face: rebuilt from the pdf when allowed and possible, else a stand-in.
fn build_face(
    fonts: &[&crate::pdf::fonts::PdfFont],
    used: &HashMap<String, BTreeSet<u32>>,
    family: &str,
) -> (RebuiltFace, FontReport) {
    let first = fonts[0];
    let mut report = FontReport {
        pdf_font: first.base_name.clone(),
        subsets: fonts.len(),
        program: first
            .program_kind
            .map(|k| k.label().to_string())
            .unwrap_or_else(|| "not embedded".into()),
        embedded_in_pdf: first.embedded(),
        ..FontReport::default()
    };
    let mut face = None;
    if first.embedded() {
        match rebuild_face(fonts, used, family) {
            Ok(rebuilt) if !may_embed(rebuilt.fs_type) => {
                report.not_used_because = Some(format!(
                    "fsType {}: {}; a stand-in is used",
                    rebuilt.fs_type.unwrap_or(0),
                    rebuilt.permission
                ));
                report.fs_type = rebuilt.fs_type;
            }
            Ok(rebuilt) => face = Some(rebuilt),
            // an unreadable program falls back to a stand-in, and says so
            Err(error) => {
                report.not_used_because = Some(format!("its program couldn’t be rebuilt ({error})"))
            }
        }
    }
    let face = face.unwrap_or_else(|| rebuild_substitute(fonts, used, family));
    let mut problems: Vec<String> = fonts
        .iter()
        .flat_map(|f| f.problems.iter().cloned())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    problems.extend(face.problems.iter().take(10).cloned());
    report.family = face.family.clone();
    report.substitute = face.substitute_for.clone();
    if report.fs_type.is_none() {
        report.fs_type = face.fs_type;
    }
    report.permission = face.permission.to_string();
    report.cubic_source = face.cubic_source;
    report.outline_error_em = face.outline_error_em;
    report.private_use_glyphs = face.private_use;
    report.problems = problems;
    (face, report)
}

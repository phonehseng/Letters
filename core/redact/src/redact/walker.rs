//! walks a content stream one operation at a time, keeping track of where
//! everything is drawn, and writes out a new stream with the parts inside
//! the redaction areas taken out.
//!
//! the walker follows the graphics state the way a viewer does: the
//! current transformation matrix (`cm`, saved and restored by `q` and `Q`),
//! line width, and the text state (font, size, spacing, scaling, leading,
//! rise, and the text and line matrices). that is what lets it know the
//! box of every glyph, image and path in page space.
//!
//! text, paths and xobjects each have their own file; this one holds the
//! shared state and the loop that sends each operation to the right place.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::rc::Rc;

use lopdf::{Dictionary, Document, Object, ObjectId};

use crate::content::Op;
use crate::fonts::Font;
use crate::geometry::{Matrix, Quad, Rect};
use crate::objects::{as_dict, dict_dict, number, resolve, NewObjects};

/// what a walk is for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WalkMode {
    /// work out the new content and build any rewritten images
    Redact,
    /// work out what would be removed, without building anything heavy
    Preview,
    /// remove nothing; record where every glyph is (for searching)
    Collect,
}

/// the redaction areas of one page and how to treat them.
#[derive(Clone, Debug)]
pub struct WalkSettings {
    pub mode: WalkMode,
    /// areas in default user space
    pub areas: Vec<Rect>,
    /// the number of each area in the whole request, for reports
    pub area_ids: Vec<usize>,
    /// fill colour as red, green and blue from 0 to 1
    pub fill: [f64; 3],
    /// clip paths that cross an area instead of removing them
    pub clip_paths: bool,
}

/// one glyph that was drawn (collect mode) or removed (other modes).
#[derive(Clone, Debug)]
pub struct GlyphRecord {
    pub text: String,
    /// the glyph's box in default user space
    pub quad: Quad,
    /// the area that removed it, if any
    pub area: Option<usize>,
}

/// how many things of each kind were taken out.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Counts {
    pub glyphs_removed: usize,
    pub images_removed: usize,
    pub images_rewritten: usize,
    pub paths_removed: usize,
    pub paths_cut: usize,
    pub paths_clipped: usize,
    pub shadings_clipped: usize,
}

/// everything a walk found out, gathered across nested forms.
#[derive(Clone, Debug, Default)]
pub struct Findings {
    pub glyphs: Vec<GlyphRecord>,
    pub counts: Counts,
    /// things removed whole because they could not be cut exactly
    pub wholesale: Vec<String>,
    /// marked content ids whose content lost something
    pub touched_mcids: Vec<i64>,
    /// images and paths removed per area, for the preview
    pub per_area: HashMap<usize, AreaTally>,
    /// images that are also used elsewhere in the file
    pub shared_images: Vec<ObjectId>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct AreaTally {
    pub images: usize,
    pub paths: usize,
}

/// loaded fonts by object id, so each font is read once per operation.
#[derive(Default)]
pub struct FontCache {
    by_id: HashMap<ObjectId, Rc<Font>>,
}

/// state shared by a page's walk and the walks of the forms inside it.
pub struct Context<'a> {
    pub doc: &'a Document,
    pub settings: &'a WalkSettings,
    pub new_objects: NewObjects,
    pub fonts: FontCache,
    pub findings: Findings,
    /// how often each object is referenced, to spot shared images
    pub reference_counts: &'a HashMap<ObjectId, usize>,
}

/// forms inside forms are followed this deep. real files rarely go past
/// three; a form that includes itself would otherwise never end.
pub const MAX_FORM_DEPTH: usize = 16;

#[derive(Clone)]
pub struct TextParams {
    pub font: Option<Rc<Font>>,
    pub font_name: Option<Vec<u8>>,
    pub size: f64,
    pub char_spacing: f64,
    pub word_spacing: f64,
    /// horizontal scaling as a fraction (Tz 100 is 1.0)
    pub scale: f64,
    pub leading: f64,
    pub rise: f64,
}

impl Default for TextParams {
    fn default() -> Self {
        Self {
            font: None,
            font_name: None,
            size: 0.0,
            char_spacing: 0.0,
            word_spacing: 0.0,
            scale: 1.0,
            leading: 0.0,
            rise: 0.0,
        }
    }
}

/// the part of the graphics state that decides where things land.
#[derive(Clone)]
pub struct GState {
    pub ctm: Matrix,
    pub line_width: f64,
    pub text: TextParams,
}

/// a marked content sequence (BMC or BDC ... EMC) that is open.
struct MarkedContent {
    /// where its BDC operation sits in the output
    out_index: usize,
    touched: bool,
    mcid: Option<i64>,
}

/// text on a line whose glyph positions stopped being known, because a
/// font's widths are missing. see `text.rs`.
#[derive(Clone, Copy)]
pub struct UncertainLine {
    pub start: Matrix,
    pub length: f64,
}

/// the result of walking one content stream.
pub struct WalkOutput {
    pub ops: Vec<Op>,
    pub changed: bool,
    /// new resources the output refers to, by category (XObject, Font)
    pub additions: BTreeMap<Vec<u8>, Vec<(Vec<u8>, Object)>>,
    /// how many `q` were still open at the end
    pub open_saves: usize,
    /// xobject names whose drawing was removed or replaced and that the
    /// new content no longer uses. they must leave the resources too, or
    /// the original image or form would stay in the file
    pub unused_xobjects: Vec<Vec<u8>>,
    /// xobject names the new content still draws
    pub kept_xobjects: HashSet<Vec<u8>>,
}

pub struct Walker<'c, 'a> {
    pub ctx: &'c mut Context<'a>,
    pub resources: Dictionary,
    pub additions: BTreeMap<Vec<u8>, Vec<(Vec<u8>, Object)>>,
    pub depth: usize,
    pub state: GState,
    saved: Vec<GState>,
    pub text_matrix: Matrix,
    pub line_matrix: Matrix,
    pub uncertain: Option<UncertainLine>,
    pub drop_rest_of_line: bool,
    pub path: crate::redact::paths::PathBuilder,
    marked: Vec<MarkedContent>,
    pub out: Vec<Op>,
    pub changed: bool,
    pub kept_xobjects: HashSet<Vec<u8>>,
    pub dropped_xobjects: HashSet<Vec<u8>>,
}

impl<'c, 'a> Walker<'c, 'a> {
    pub fn new(ctx: &'c mut Context<'a>, resources: Dictionary, ctm: Matrix, depth: usize) -> Self {
        Self {
            ctx,
            resources,
            additions: BTreeMap::new(),
            depth,
            state: GState {
                ctm,
                line_width: 1.0,
                text: TextParams::default(),
            },
            saved: Vec::new(),
            text_matrix: Matrix::IDENTITY,
            line_matrix: Matrix::IDENTITY,
            uncertain: None,
            drop_rest_of_line: false,
            path: Default::default(),
            marked: Vec::new(),
            out: Vec::new(),
            changed: false,
            kept_xobjects: HashSet::new(),
            dropped_xobjects: HashSet::new(),
        }
    }

    pub fn doc(&self) -> &'a Document {
        self.ctx.doc
    }

    pub fn settings(&self) -> &'a WalkSettings {
        self.ctx.settings
    }

    /// walks every operation and returns the new content.
    pub fn run(mut self, ops: Vec<Op>) -> Result<WalkOutput, String> {
        for op in ops {
            self.handle(op)?;
        }
        self.flush_path();
        while let Some(entry) = self.marked.pop() {
            self.close_marked(entry);
        }
        let unused_xobjects = self
            .dropped_xobjects
            .iter()
            .filter(|name| !self.kept_xobjects.contains(*name))
            .cloned()
            .collect();
        Ok(WalkOutput {
            ops: self.out,
            changed: self.changed,
            additions: self.additions,
            open_saves: self.saved.len(),
            unused_xobjects,
            kept_xobjects: self.kept_xobjects,
        })
    }

    fn handle(&mut self, op: Op) -> Result<(), String> {
        let name = op.name().to_owned();
        if crate::redact::paths::is_path_operator(&name) {
            self.path.push(op, &self.state.ctm);
            return Ok(());
        }
        if crate::redact::paths::is_paint_operator(&name) {
            self.paint(op);
            return Ok(());
        }
        // anything else ends a path that was never painted
        self.flush_path();
        match name.as_str() {
            "q" => {
                self.saved.push(self.state.clone());
                self.emit(op);
            }
            "Q" => {
                // a Q with nothing saved is ignored by viewers, so it is dropped
                if let Some(state) = self.saved.pop() {
                    self.state = state;
                    self.emit(op);
                }
            }
            "cm" => {
                if let Some(matrix) = matrix_operand(self.doc(), op.operands()) {
                    self.state.ctm = matrix.then(&self.state.ctm);
                }
                self.emit(op);
            }
            "w" => {
                if let Some(width) = op.operands().first().and_then(|o| number(self.doc(), o)) {
                    self.state.line_width = width;
                }
                self.emit(op);
            }
            "gs" => {
                self.apply_ext_gstate(op.operands());
                self.emit(op);
            }
            "BMC" | "BDC" => self.open_marked(op),
            "EMC" => {
                self.emit(op);
                if let Some(entry) = self.marked.pop() {
                    self.close_marked(entry);
                }
            }
            "Do" => self.draw_xobject(op)?,
            "BI" => self.draw_inline_image(op),
            "sh" => self.draw_shading(op),
            _ if self.handle_text(&op) => {}
            _ => self.emit(op),
        }
        Ok(())
    }

    pub fn emit(&mut self, op: Op) {
        self.out.push(op);
    }

    /// notes that something was removed or changed, which marks every
    /// open marked content sequence as touched.
    pub fn touch(&mut self) {
        self.changed = true;
        for entry in &mut self.marked {
            entry.touched = true;
        }
    }

    /// the first area (by its number in the request) that a box overlaps.
    pub fn first_area_hit(&self, quad: &Quad) -> Option<usize> {
        let settings = self.settings();
        settings
            .areas
            .iter()
            .position(|area| quad.overlaps(area))
            .map(|index| settings.area_ids[index])
    }

    /// the areas that overlap a rectangle in page space.
    pub fn areas_hitting(&self, bounds: &Rect) -> Vec<Rect> {
        self.settings()
            .areas
            .iter()
            .filter(|area| area.overlaps(bounds))
            .copied()
            .collect()
    }

    pub fn tally(&mut self, area: Option<usize>, image: bool) {
        if let Some(area) = area {
            let tally = self.ctx.findings.per_area.entry(area).or_default();
            if image {
                tally.images += 1;
            } else {
                tally.paths += 1;
            }
        }
    }

    /// a resource stored as its own object, e.g. XObject /Im1, with its id.
    /// streams (images and forms) are always stored this way, so they can
    /// be read straight from the document instead of being copied.
    pub fn referenced_resource(
        &self,
        category: &[u8],
        name: &[u8],
    ) -> Option<(ObjectId, &'a Object)> {
        let doc = self.doc();
        let entry = match self.resources.get(category).ok().map(|o| resolve(doc, o)) {
            Some(Object::Dictionary(dict)) => dict.get(name).ok()?,
            _ => return None,
        };
        let mut id = entry.as_reference().ok()?;
        // a reference to a reference is followed to the end
        for _ in 0..32 {
            match doc.objects.get(&id)? {
                Object::Reference(next) => id = *next,
                object => return Some((id, object)),
            }
        }
        None
    }

    /// a resource that may be stored directly in the resources dictionary.
    pub fn resource_owned(&self, category: &[u8], name: &[u8]) -> Option<Object> {
        let doc = self.doc();
        match self.resources.get(category).ok().map(|o| resolve(doc, o)) {
            Some(Object::Dictionary(dict)) => dict.get(name).ok().map(|o| resolve(doc, o).clone()),
            _ => None,
        }
    }

    /// adds a new resource under a name not used yet, and returns the name.
    pub fn add_resource(&mut self, category: &[u8], object: Object) -> Vec<u8> {
        let doc = self.doc();
        let existing: Vec<Vec<u8>> =
            match self.resources.get(category).ok().map(|o| resolve(doc, o)) {
                Some(Object::Dictionary(dict)) => dict.iter().map(|(key, _)| key.clone()).collect(),
                _ => Vec::new(),
            };
        let added = self.additions.entry(category.to_vec()).or_default();
        let mut number = added.len() + 1;
        let name = loop {
            let candidate = format!("LR{number}").into_bytes();
            let taken =
                existing.contains(&candidate) || added.iter().any(|(key, _)| *key == candidate);
            if !taken {
                break candidate;
            }
            number += 1;
        };
        added.push((name.clone(), object));
        name
    }

    fn apply_ext_gstate(&mut self, operands: &[Object]) {
        let Some(Object::Name(name)) = operands.first() else {
            return;
        };
        let Some(gs) = self.resource_owned(b"ExtGState", name) else {
            return;
        };
        let Some(gs) = as_dict(&gs) else {
            return;
        };
        let doc = self.doc();
        if let Some(width) = gs.get(b"LW").ok().and_then(|o| number(doc, o)) {
            self.state.line_width = width;
        }
        if let Some(Object::Array(font)) = gs.get(b"Font").ok().map(|o| resolve(doc, o)) {
            if let (Some(Object::Reference(id)), Some(size)) = (font.first(), font.get(1)) {
                self.state.text.font = Some(self.load_font_by_id(*id));
                self.state.text.font_name = None;
                self.state.text.size = number(doc, size).unwrap_or(0.0);
            }
        }
    }

    /// loads (or finds in the cache) the font stored as object `id`.
    pub fn load_font_by_id(&mut self, id: ObjectId) -> Rc<Font> {
        if let Some(font) = self.ctx.fonts.by_id.get(&id) {
            return font.clone();
        }
        let doc = self.doc();
        let font = match doc
            .objects
            .get(&id)
            .map(|o| resolve(doc, o))
            .and_then(as_dict)
        {
            Some(dict) => Font::load(doc, dict),
            None => Font::unknown("a missing font"),
        };
        let font = Rc::new(font);
        self.ctx.fonts.by_id.insert(id, font.clone());
        font
    }

    /// loads the font a `Tf` names from the resources.
    pub fn load_font_by_name(&mut self, name: &[u8]) -> Rc<Font> {
        let doc = self.doc();
        let entry = match self.resources.get(b"Font").ok().map(|o| resolve(doc, o)) {
            Some(Object::Dictionary(fonts)) => fonts.get(name).ok().cloned(),
            _ => None,
        };
        match entry {
            Some(Object::Reference(id)) => self.load_font_by_id(id),
            Some(Object::Dictionary(dict)) => Rc::new(Font::load(doc, &dict)),
            _ => Rc::new(Font::unknown(&String::from_utf8_lossy(name))),
        }
    }

    fn open_marked(&mut self, op: Op) {
        let mcid = self.marked_content_id(op.operands());
        self.marked.push(MarkedContent {
            out_index: self.out.len(),
            touched: false,
            mcid,
        });
        self.emit(op);
    }

    fn marked_content_id(&self, operands: &[Object]) -> Option<i64> {
        let props = self.marked_properties(operands)?;
        props.get(b"MCID").ok()?.as_i64().ok()
    }

    /// the property list of a BDC, written inline or named in /Properties.
    fn marked_properties(&self, operands: &[Object]) -> Option<Dictionary> {
        match operands.get(1)? {
            Object::Dictionary(dict) => Some(dict.clone()),
            Object::Name(name) => as_dict(&self.resource_owned(b"Properties", name)?).cloned(),
            _ => None,
        }
    }

    /// a sequence whose content lost something must not keep a text
    /// alternative that still spells the removed content (/ActualText,
    /// /Alt, /E), so those are taken off its property list.
    fn close_marked(&mut self, entry: MarkedContent) {
        if !entry.touched {
            return;
        }
        if let Some(mcid) = entry.mcid {
            self.ctx.findings.touched_mcids.push(mcid);
        }
        let Some(Op::Operator { name, operands }) = self.out.get(entry.out_index).cloned() else {
            return;
        };
        if name != "BDC" || operands.first() == Some(&Object::Name(b"OC".to_vec())) {
            return;
        }
        let Some(mut props) = self.marked_properties(&operands) else {
            return;
        };
        let mut removed_any = false;
        for key in TEXT_ALTERNATIVE_KEYS {
            removed_any |= props.remove(key).is_some();
        }
        if removed_any {
            let mut operands = operands;
            operands[1] = Object::Dictionary(props);
            self.out[entry.out_index] = Op::Operator { name, operands };
        }
    }

    /// walks a nested form with this walker's shared state. a form that
    /// borrows this walker's resources (`own_resources` false) uses names
    /// from them, so the names it still draws count as kept here too.
    pub fn walk_child(
        &mut self,
        resources: Dictionary,
        own_resources: bool,
        ctm: Matrix,
        ops: Vec<Op>,
    ) -> Result<WalkOutput, String> {
        let child = Walker::new(self.ctx, resources, ctm, self.depth + 1);
        let output = child.run(ops)?;
        if !own_resources {
            self.kept_xobjects
                .extend(output.kept_xobjects.iter().cloned());
        }
        Ok(output)
    }

    /// the resources a form uses: its own, or (for old files) the page's,
    /// and whether they are its own.
    pub fn form_resources(&self, form: &Dictionary) -> (Dictionary, bool) {
        let doc = self.doc();
        match dict_dict(doc, form, b"Resources") {
            Some(own) => (own.clone(), true),
            None => (self.resources.clone(), false),
        }
    }

    /// records what happened to a drawn xobject name.
    pub fn note_xobject(&mut self, name: &[u8], kept: bool) {
        if kept {
            self.kept_xobjects.insert(name.to_vec());
        } else {
            self.dropped_xobjects.insert(name.to_vec());
        }
    }
}

/// keys of a property list or structure element that hold text standing
/// in for the content: replacement text, alternate description and the
/// expansion of an abbreviation.
pub const TEXT_ALTERNATIVE_KEYS: [&[u8]; 3] = [b"ActualText", b"Alt", b"E"];

/// six numbers as a matrix, from the operands of `cm` or `Tm`.
pub fn matrix_operand(doc: &Document, operands: &[Object]) -> Option<Matrix> {
    let numbers: Vec<f64> = operands.iter().filter_map(|o| number(doc, o)).collect();
    Matrix::from_numbers(&numbers)
}

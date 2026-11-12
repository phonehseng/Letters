//! helpers shared by the integration tests: building small tagged and
//! untagged pdfs by hand, checking them, and drawing their pages.
//!
//! documents are built object by object with lopdf so each test says
//! exactly what is in the file, without depending on any pdf writer.

// each test file uses only some of these helpers
#![allow(dead_code)]

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use letters_a11y::{Rect, RegionRenderer, Report, RgbaImage, Severity};
use letters_pdf::{DocumentId, RenderWorker, TileKey};
use lopdf::{Dictionary, Document, Object, ObjectId, Stream, StringFormat};

pub const WIDTH: f32 = 612.0;
pub const HEIGHT: f32 = 792.0;

pub fn name(text: &str) -> Object {
    Object::Name(text.as_bytes().to_vec())
}

pub fn string(text: &str) -> Object {
    Object::String(text.as_bytes().to_vec(), StringFormat::Literal)
}

pub fn numbers(values: &[f32]) -> Object {
    Object::Array(values.iter().map(|&v| Object::Real(v)).collect())
}

// ---- documents -----------------------------------------------------------------

/// one page of a test document.
#[derive(Clone)]
pub struct PageSpec {
    pub width: f32,
    pub height: f32,
    pub rotate: i64,
    pub crop: Option<[f32; 4]>,
    pub content: String,
}

impl PageSpec {
    pub fn new(content: &str) -> Self {
        Self {
            width: WIDTH,
            height: HEIGHT,
            rotate: 0,
            crop: None,
            content: content.to_owned(),
        }
    }

    pub fn sized(mut self, width: f32, height: f32) -> Self {
        self.width = width;
        self.height = height;
        self
    }

    pub fn rotated(mut self, degrees: i64) -> Self {
        self.rotate = degrees;
        self
    }

    pub fn cropped(mut self, crop: [f32; 4]) -> Self {
        self.crop = Some(crop);
        self
    }
}

/// builds a document page by page. every page can use:
/// - /F1: an embedded type 3 font whose glyphs are boxes, with a
///   ToUnicode map, so it passes the font rules
/// - /F2: helvetica, not embedded
/// - /F3: helvetica bold, not embedded
/// - /F4: the box font again, but its ToUnicode map reads A to Z as
///   the greek capitals alpha to omega, for language tests
/// - /F5: a symbolic truetype font with no encoding and no ToUnicode,
///   so its text can't be turned into characters
/// - /Im1: a small picture
pub struct Builder {
    pub doc: Document,
    pages_root: ObjectId,
    resources: ObjectId,
    pub pages: Vec<ObjectId>,
}

impl Default for Builder {
    fn default() -> Self {
        Self::new()
    }
}

impl Builder {
    pub fn new() -> Self {
        let mut doc = Document::with_version("1.7");
        let pages_root = doc.new_object_id();
        let plain_font = box_font(&mut doc, "<20> <7E> <0020>");
        let greek_font = box_font(&mut doc, "<41> <5A> <0391>");
        let symbolic = symbolic_font(&mut doc);
        let helvetica = standard_font(&mut doc, "Helvetica");
        let helvetica_bold = standard_font(&mut doc, "Helvetica-Bold");
        let image = small_image(&mut doc);
        let mut fonts = Dictionary::new();
        fonts.set("F1", Object::Reference(plain_font));
        fonts.set("F2", Object::Reference(helvetica));
        fonts.set("F3", Object::Reference(helvetica_bold));
        fonts.set("F4", Object::Reference(greek_font));
        fonts.set("F5", Object::Reference(symbolic));
        let mut xobjects = Dictionary::new();
        xobjects.set("Im1", Object::Reference(image));
        let mut resources = Dictionary::new();
        resources.set("Font", Object::Dictionary(fonts));
        resources.set("XObject", Object::Dictionary(xobjects));
        let resources = doc.add_object(resources);
        Self {
            doc,
            pages_root,
            resources,
            pages: Vec::new(),
        }
    }

    pub fn page(&mut self, content: &str) -> usize {
        self.page_spec(PageSpec::new(content))
    }

    pub fn page_spec(&mut self, spec: PageSpec) -> usize {
        let content = self
            .doc
            .add_object(Stream::new(Dictionary::new(), spec.content.into_bytes()));
        let mut page = Dictionary::new();
        page.set("Type", name("Page"));
        page.set("Parent", Object::Reference(self.pages_root));
        page.set("MediaBox", numbers(&[0.0, 0.0, spec.width, spec.height]));
        if let Some(crop) = spec.crop {
            page.set("CropBox", numbers(&crop));
        }
        if spec.rotate != 0 {
            page.set("Rotate", Object::Integer(spec.rotate));
        }
        page.set("Contents", Object::Reference(content));
        page.set("Resources", Object::Reference(self.resources));
        let id = self.doc.add_object(page);
        self.pages.push(id);
        self.pages.len() - 1
    }

    /// adds an annotation to a page and answers its object.
    pub fn annotation(&mut self, page: usize, mut dict: Dictionary) -> ObjectId {
        dict.set("P", Object::Reference(self.pages[page]));
        let id = self.doc.add_object(dict);
        let page_dict = self.doc.get_dictionary_mut(self.pages[page]).unwrap();
        let mut list = page_dict
            .get(b"Annots")
            .ok()
            .and_then(|a| a.as_array().ok().cloned())
            .unwrap_or_default();
        list.push(Object::Reference(id));
        page_dict.set("Annots", Object::Array(list));
        id
    }

    pub fn build(mut self) -> Document {
        let mut root = Dictionary::new();
        root.set("Type", name("Pages"));
        root.set("Count", Object::Integer(self.pages.len() as i64));
        root.set(
            "Kids",
            Object::Array(self.pages.iter().map(|&p| Object::Reference(p)).collect()),
        );
        self.doc
            .objects
            .insert(self.pages_root, Object::Dictionary(root));
        let mut catalog = Dictionary::new();
        catalog.set("Type", name("Catalog"));
        catalog.set("Pages", Object::Reference(self.pages_root));
        let catalog = self.doc.add_object(catalog);
        self.doc.trailer.set("Root", Object::Reference(catalog));
        self.doc
    }
}

fn standard_font(doc: &mut Document, base: &str) -> ObjectId {
    let mut font = Dictionary::new();
    font.set("Type", name("Font"));
    font.set("Subtype", name("Type1"));
    font.set("BaseFont", name(base));
    font.set("Encoding", name("WinAnsiEncoding"));
    doc.add_object(font)
}

/// a type 3 font: every printable character is a box 0.5 wide and 0.7
/// tall, drawn in the current fill colour, plus a ToUnicode map so the
/// codes read back as the characters typed.
fn box_font(doc: &mut Document, range: &str) -> ObjectId {
    let glyph = doc.add_object(Stream::new(
        Dictionary::new(),
        b"600 0 0 0 500 700 d1 0 0 500 700 re f".to_vec(),
    ));
    let space = doc.add_object(Stream::new(Dictionary::new(), b"250 0 d0".to_vec()));
    let mut procs = Dictionary::new();
    procs.set("box", Object::Reference(glyph));
    procs.set("space", Object::Reference(space));
    let mut differences = vec![Object::Integer(32), name("space")];
    differences.extend((33..=126).map(|_| name("box")));
    let mut encoding = Dictionary::new();
    encoding.set("Type", name("Encoding"));
    encoding.set("Differences", Object::Array(differences));
    let cmap = format!(
        "/CIDInit /ProcSet findresource begin\n12 dict begin\nbegincmap\n\
        /CIDSystemInfo << /Registry (Adobe) /Ordering (UCS) /Supplement 0 >> def\n\
        /CMapName /Adobe-Identity-UCS def\n/CMapType 2 def\n\
        1 begincodespacerange\n<00> <FF>\nendcodespacerange\n\
        1 beginbfrange\n{range}\nendbfrange\n\
        endcmap\nCMapName currentdict /CMap defineresource pop\nend\nend\n"
    );
    let to_unicode = doc.add_object(Stream::new(Dictionary::new(), cmap.as_bytes().to_vec()));
    let mut widths = vec![Object::Integer(250)];
    widths.extend((33..=126).map(|_| Object::Integer(600)));
    let mut font = Dictionary::new();
    font.set("Type", name("Font"));
    font.set("Subtype", name("Type3"));
    font.set("FontBBox", numbers(&[0.0, 0.0, 600.0, 700.0]));
    font.set("FontMatrix", numbers(&[0.001, 0.0, 0.0, 0.001, 0.0, 0.0]));
    font.set("CharProcs", Object::Dictionary(procs));
    font.set("Encoding", Object::Dictionary(encoding));
    font.set("FirstChar", Object::Integer(32));
    font.set("LastChar", Object::Integer(126));
    font.set("Widths", Object::Array(widths));
    font.set("ToUnicode", Object::Reference(to_unicode));
    font.set("Resources", Object::Dictionary(Dictionary::new()));
    doc.add_object(font)
}

fn symbolic_font(doc: &mut Document) -> ObjectId {
    let mut descriptor = Dictionary::new();
    descriptor.set("Type", name("FontDescriptor"));
    descriptor.set("FontName", name("Wingdings"));
    descriptor.set("Flags", Object::Integer(4));
    let descriptor = doc.add_object(descriptor);
    let mut font = Dictionary::new();
    font.set("Type", name("Font"));
    font.set("Subtype", name("TrueType"));
    font.set("BaseFont", name("Wingdings"));
    font.set("FontDescriptor", Object::Reference(descriptor));
    doc.add_object(font)
}

/// a 2 by 2 picture: red, green, blue and yellow squares, with no white,
/// so tests can tell the picture from the paper.
fn small_image(doc: &mut Document) -> ObjectId {
    let mut dict = Dictionary::new();
    dict.set("Type", name("XObject"));
    dict.set("Subtype", name("Image"));
    dict.set("Width", Object::Integer(2));
    dict.set("Height", Object::Integer(2));
    dict.set("ColorSpace", name("DeviceRGB"));
    dict.set("BitsPerComponent", Object::Integer(8));
    let pixels = vec![255, 0, 0, 0, 255, 0, 0, 0, 255, 255, 255, 0];
    doc.add_object(Stream::new(dict, pixels).with_compression(false))
}

// ---- content -------------------------------------------------------------------

/// one line of text, placed by its baseline measured from the top of a
/// letter page.
pub fn text(font: &str, size: f32, x: f32, from_top: f32, words: &str) -> String {
    format!(
        "BT /{font} {size} Tf {x} {} Td ({}) Tj ET\n",
        HEIGHT - from_top,
        words
            .replace('\\', "\\\\")
            .replace('(', "\\(")
            .replace(')', "\\)")
    )
}

/// content wrapped in a tagged marked-content sequence.
pub fn marked(role: &str, mcid: i64, content: &str) -> String {
    format!("/{role} <</MCID {mcid}>> BDC\n{content}EMC\n")
}

pub fn artifact(content: &str) -> String {
    format!("/Artifact BMC\n{content}EMC\n")
}

/// the small picture, drawn `width` by `height` with its top left corner
/// at x, measured from the top.
pub fn picture(x: f32, from_top: f32, width: f32, height: f32) -> String {
    format!(
        "q {width} 0 0 {height} {x} {} cm /Im1 Do Q\n",
        HEIGHT - from_top - height
    )
}

/// a stroked rectangle, given by its top left corner from the top.
pub fn frame(x: f32, from_top: f32, width: f32, height: f32) -> String {
    format!(
        "0 G 1 w {x} {} {width} {height} re S\n",
        HEIGHT - from_top - height
    )
}

// ---- tags ------------------------------------------------------------------------

/// a kid of a structure element in a test tree.
pub enum Kid {
    Tag(Tag),
    /// page index, mcid
    Content(usize, i64),
    /// page index, annotation
    Object(usize, ObjectId),
}

/// a structure element to write.
pub struct Tag {
    pub role: String,
    pub kids: Vec<Kid>,
    pub entries: Vec<(String, Object)>,
}

pub fn tag(role: &str, kids: Vec<Kid>) -> Kid {
    Kid::Tag(Tag {
        role: role.to_owned(),
        kids,
        entries: Vec::new(),
    })
}

/// a tag owning content: `role` with mcids on one page.
pub fn leaf(role: &str, page: usize, mcids: &[i64]) -> Kid {
    tag(role, mcids.iter().map(|&m| Kid::Content(page, m)).collect())
}

/// adds an entry such as /Alt to a tag.
pub fn with(kid: Kid, key: &str, value: Object) -> Kid {
    match kid {
        Kid::Tag(mut t) => {
            t.entries.push((key.to_owned(), value));
            Kid::Tag(t)
        }
        other => other,
    }
}

/// table attributes with a scope.
pub fn scope(scope: &str) -> Object {
    let mut dict = Dictionary::new();
    dict.set("O", name("Table"));
    dict.set("Scope", name(scope));
    Object::Dictionary(dict)
}

/// writes a structure tree: a Document element holding `top`, the parent
/// tree, page and annotation keys, and MarkInfo /Marked true.
pub fn add_tags(doc: &mut Document, pages: &[ObjectId], top: Vec<Kid>) {
    let root = doc.new_object_id();
    let document = doc.new_object_id();
    // page index → owners by mcid; annotation → owner
    let mut page_owners: Vec<Vec<(i64, ObjectId)>> = vec![Vec::new(); pages.len()];
    let mut annotation_owners: Vec<(ObjectId, ObjectId)> = Vec::new();
    let kids = write_kids(
        doc,
        document,
        top,
        pages,
        &mut page_owners,
        &mut annotation_owners,
    );
    let mut element = Dictionary::new();
    element.set("Type", name("StructElem"));
    element.set("S", name("Document"));
    element.set("P", Object::Reference(root));
    element.set("K", Object::Array(kids));
    doc.objects.insert(document, Object::Dictionary(element));

    let mut nums = Vec::new();
    for (index, owners) in page_owners.iter().enumerate() {
        let size = owners.iter().map(|(m, _)| *m + 1).max().unwrap_or(0) as usize;
        let mut list = vec![Object::Null; size];
        for (mcid, owner) in owners {
            list[*mcid as usize] = Object::Reference(*owner);
        }
        nums.push(Object::Integer(index as i64));
        nums.push(Object::Array(list));
        doc.get_dictionary_mut(pages[index])
            .unwrap()
            .set("StructParents", Object::Integer(index as i64));
    }
    let mut next_key = pages.len() as i64;
    for (annotation, owner) in annotation_owners {
        nums.push(Object::Integer(next_key));
        nums.push(Object::Reference(owner));
        doc.get_dictionary_mut(annotation)
            .unwrap()
            .set("StructParent", Object::Integer(next_key));
        next_key += 1;
    }
    let mut parent_tree = Dictionary::new();
    parent_tree.set("Nums", Object::Array(nums));
    let parent_tree = doc.add_object(parent_tree);
    let mut root_dict = Dictionary::new();
    root_dict.set("Type", name("StructTreeRoot"));
    root_dict.set("K", Object::Reference(document));
    root_dict.set("ParentTree", Object::Reference(parent_tree));
    root_dict.set("ParentTreeNextKey", Object::Integer(next_key));
    doc.objects.insert(root, Object::Dictionary(root_dict));
    let catalog = doc.catalog_mut().unwrap();
    catalog.set("StructTreeRoot", Object::Reference(root));
    let mut mark_info = Dictionary::new();
    mark_info.set("Marked", Object::Boolean(true));
    catalog.set("MarkInfo", Object::Dictionary(mark_info));
}

fn write_kids(
    doc: &mut Document,
    parent: ObjectId,
    kids: Vec<Kid>,
    pages: &[ObjectId],
    page_owners: &mut Vec<Vec<(i64, ObjectId)>>,
    annotation_owners: &mut Vec<(ObjectId, ObjectId)>,
) -> Vec<Object> {
    kids.into_iter()
        .map(|kid| match kid {
            Kid::Tag(tag) => {
                let id = doc.new_object_id();
                let first_page = tag.kids.iter().find_map(|k| match k {
                    Kid::Content(page, _) | Kid::Object(page, _) => Some(*page),
                    _ => None,
                });
                let mut element = Dictionary::new();
                element.set("Type", name("StructElem"));
                element.set("S", name(&tag.role));
                element.set("P", Object::Reference(parent));
                if let Some(page) = first_page {
                    element.set("Pg", Object::Reference(pages[page]));
                }
                for (key, value) in tag.entries {
                    element.set(key, value);
                }
                let mut own_kids = Vec::new();
                for kid in tag.kids {
                    match kid {
                        Kid::Content(page, mcid) => {
                            page_owners[page].push((mcid, id));
                            if Some(page) == first_page {
                                own_kids.push(Object::Integer(mcid));
                            } else {
                                let mut mcr = Dictionary::new();
                                mcr.set("Type", name("MCR"));
                                mcr.set("Pg", Object::Reference(pages[page]));
                                mcr.set("MCID", Object::Integer(mcid));
                                own_kids.push(Object::Dictionary(mcr));
                            }
                        }
                        Kid::Object(page, object) => {
                            annotation_owners.push((object, id));
                            let mut objr = Dictionary::new();
                            objr.set("Type", name("OBJR"));
                            objr.set("Pg", Object::Reference(pages[page]));
                            objr.set("Obj", Object::Reference(object));
                            own_kids.push(Object::Dictionary(objr));
                        }
                        Kid::Tag(child) => {
                            own_kids.extend(write_kids(
                                doc,
                                id,
                                vec![Kid::Tag(child)],
                                pages,
                                page_owners,
                                annotation_owners,
                            ));
                        }
                    }
                }
                element.set("K", Object::Array(own_kids));
                doc.objects.insert(id, Object::Dictionary(element));
                Object::Reference(id)
            }
            Kid::Content(_, mcid) => Object::Integer(mcid),
            Kid::Object(_, object) => Object::Reference(object),
        })
        .collect()
}

/// sets the language, the title (information and xmp) and DisplayDocTitle.
pub fn add_metadata(doc: &mut Document, language: &str, title: &str) {
    let mut info = Dictionary::new();
    info.set("Title", string(title));
    let info = doc.add_object(info);
    doc.trailer.set("Info", Object::Reference(info));
    let packet = format!(
        "<?xpacket begin=\"\" id=\"W5M0MpCehiHzreSzNTczkc9d\"?><x:xmpmeta xmlns:x=\"adobe:ns:meta/\">\
         <rdf:RDF xmlns:rdf=\"http://www.w3.org/1999/02/22-rdf-syntax-ns#\">\
         <rdf:Description rdf:about=\"\" xmlns:dc=\"http://purl.org/dc/elements/1.1/\">\
         <dc:title><rdf:Alt><rdf:li xml:lang=\"x-default\">{title}</rdf:li></rdf:Alt></dc:title>\
         </rdf:Description></rdf:RDF></x:xmpmeta><?xpacket end=\"w\"?>"
    );
    let mut metadata = Dictionary::new();
    metadata.set("Type", name("Metadata"));
    metadata.set("Subtype", name("XML"));
    let metadata =
        doc.add_object(Stream::new(metadata, packet.into_bytes()).with_compression(false));
    let catalog = doc.catalog_mut().unwrap();
    catalog.set("Lang", string(language));
    catalog.set("Metadata", Object::Reference(metadata));
    let mut preferences = Dictionary::new();
    preferences.set("DisplayDocTitle", Object::Boolean(true));
    catalog.set("ViewerPreferences", Object::Dictionary(preferences));
}

/// a one-page tagged document that passes every machine check: a heading
/// and a paragraph in the embedded box font, with language and title.
pub fn good_document() -> Document {
    good_document_with(|_| {})
}

/// like `good_document`, with a chance to change the builder before tags
/// are written. the page content is fixed: mcid 0 is the heading, 1 the
/// paragraph.
pub fn good_document_with(change: impl FnOnce(&mut Builder)) -> Document {
    let mut builder = Builder::new();
    let content = format!(
        "{}{}",
        marked("H1", 0, &text("F1", 24.0, 72.0, 100.0, "Rivers")),
        marked(
            "P",
            1,
            &text("F1", 12.0, 72.0, 140.0, "Rivers carry water to the sea.")
        )
    );
    builder.page(&content);
    change(&mut builder);
    let pages = builder.pages.clone();
    let mut doc = builder.build();
    add_tags(
        &mut doc,
        &pages,
        vec![leaf("H1", 0, &[0]), leaf("P", 0, &[1])],
    );
    add_metadata(&mut doc, "en-US", "Rivers");
    doc
}

// ---- checking --------------------------------------------------------------------

pub fn check(doc: &Document) -> Report {
    letters_a11y::check(doc, None).unwrap()
}

/// the ids of everything a program found wrong (not the checks by hand).
pub fn problems(report: &Report) -> BTreeSet<String> {
    report
        .findings
        .iter()
        .filter(|f| f.severity != Severity::CheckByHand)
        .map(|f| f.id.clone())
        .collect()
}

pub fn problem_ids(doc: &Document) -> BTreeSet<String> {
    problems(&check(doc))
}

pub fn set(ids: &[&str]) -> BTreeSet<String> {
    ids.iter().map(|id| id.to_string()).collect()
}

/// saves and loads again, the way a file on disk would come back.
pub fn reload(doc: &Document) -> Document {
    let mut copy = doc.clone();
    let mut bytes = Vec::new();
    copy.save_to(&mut bytes).unwrap();
    Document::load_mem(&bytes).unwrap()
}

// ---- drawing ---------------------------------------------------------------------

/// where the pdfium library lives for tests (see core/pdf/tests/support).
pub fn pdfium_dir() -> PathBuf {
    let dir = match std::env::var_os("LETTERS_PDFIUM_DIR") {
        Some(dir) => PathBuf::from(dir),
        None => Path::new(env!("CARGO_MANIFEST_DIR")).join("../../vendor/pdfium"),
    };
    assert!(
        dir.exists(),
        "pdfium was not found in {}. run tools/fetch-pdfium first.",
        dir.display()
    );
    dir
}

/// one worker for the whole test run; loading pdfium once is enough.
fn worker() -> &'static RenderWorker {
    static WORKER: OnceLock<RenderWorker> = OnceLock::new();
    WORKER.get_or_init(|| RenderWorker::start(&pdfium_dir(), 16 * 1024 * 1024).unwrap())
}

fn temp_file(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join("letters-a11y-tests");
    std::fs::create_dir_all(&dir).unwrap();
    dir.join(format!("{name}-{}.pdf", std::process::id()))
}

/// draws a document with pdfium, the way the app does, for as long as it lives.
pub struct Pdfium {
    id: DocumentId,
    path: PathBuf,
    sizes: Vec<(f32, f32)>,
}

impl Pdfium {
    pub fn open(doc: &Document, name: &str) -> Self {
        let path = temp_file(name);
        let mut copy = doc.clone();
        let mut bytes = Vec::new();
        copy.save_to(&mut bytes).unwrap();
        std::fs::write(&path, bytes).unwrap();
        let info = worker().open(&path, None).unwrap();
        Self {
            id: info.id,
            path,
            sizes: info
                .page_sizes
                .iter()
                .map(|s| (s.width_pt, s.height_pt))
                .collect(),
        }
    }

    pub fn page_size(&self, page: u32) -> (f32, f32) {
        self.sizes[page as usize]
    }

    /// one tile decoded to rgba.
    fn tile(&self, page: u32, scale_milli: u32, column: u32, row: u32) -> (u32, u32, Vec<u8>) {
        let png = worker()
            .tile(TileKey {
                document: self.id,
                page,
                scale_milli,
                column,
                row,
            })
            .unwrap();
        let mut reader = png::Decoder::new(std::io::Cursor::new(&png[..]))
            .read_info()
            .unwrap();
        let mut rgba = vec![0; reader.output_buffer_size().unwrap()];
        let info = reader.next_frame(&mut rgba).unwrap();
        (info.width, info.height, rgba)
    }

    /// every pixel of a page at one pixel per point, tile after tile.
    pub fn page_pixels(&self, page: u32) -> Vec<u8> {
        let (width, height) = self.page_size(page);
        let columns = (width.ceil() as u32).div_ceil(512);
        let rows = (height.ceil() as u32).div_ceil(512);
        let mut pixels = Vec::new();
        for row in 0..rows {
            for column in 0..columns {
                pixels.extend(self.tile(page, 1000, column, row).2);
            }
        }
        pixels
    }
}

impl Drop for Pdfium {
    fn drop(&mut self) {
        worker().close(self.id);
        let _ = std::fs::remove_file(&self.path);
    }
}

impl RegionRenderer for Pdfium {
    fn render_region(&mut self, page: u32, rect: Rect, scale: f32) -> Option<RgbaImage> {
        let (width, height) = *self.sizes.get(page as usize)?;
        let page_pixels = (
            (width * scale).ceil() as u32,
            (height * scale).ceil() as u32,
        );
        let x0 = ((rect.x * scale).floor().max(0.0) as u32).min(page_pixels.0 - 1);
        let y0 = ((rect.y * scale).floor().max(0.0) as u32).min(page_pixels.1 - 1);
        let x1 = ((rect.right() * scale).ceil() as u32).clamp(x0 + 1, page_pixels.0);
        let y1 = ((rect.bottom() * scale).ceil() as u32).clamp(y0 + 1, page_pixels.1);
        let (out_width, out_height) = (x1 - x0, y1 - y0);
        let mut pixels = vec![0u8; (out_width * out_height * 4) as usize];
        let scale_milli = (scale * 1000.0).round() as u32;
        for row in y0 / 512..=(y1 - 1) / 512 {
            for column in x0 / 512..=(x1 - 1) / 512 {
                let (tile_width, tile_height, tile) = self.tile(page, scale_milli, column, row);
                for ty in 0..tile_height {
                    for tx in 0..tile_width {
                        let (px, py) = (column * 512 + tx, row * 512 + ty);
                        if px < x0 || px >= x1 || py < y0 || py >= y1 {
                            continue;
                        }
                        let from = ((ty * tile_width + tx) * 4) as usize;
                        let to = (((py - y0) * out_width + (px - x0)) * 4) as usize;
                        pixels[to..to + 4].copy_from_slice(&tile[from..from + 4]);
                    }
                }
            }
        }
        Some(RgbaImage {
            width: out_width,
            height: out_height,
            pixels,
        })
    }
}

/// every page's pixels, for checking that a fix changed nothing visible.
pub fn render_all(doc: &Document, name: &str) -> Vec<Vec<u8>> {
    let pdfium = Pdfium::open(doc, name);
    (0..pdfium.sizes.len() as u32)
        .map(|page| pdfium.page_pixels(page))
        .collect()
}

/// checks that a document still loads after saving and draws exactly as before.
pub fn assert_same_pixels(before: &Document, after: &Document, name: &str) {
    let reloaded = reload(after);
    assert_eq!(
        render_all(before, &format!("{name}-before")),
        render_all(&reloaded, &format!("{name}-after")),
        "the fix changed what the page looks like"
    );
}

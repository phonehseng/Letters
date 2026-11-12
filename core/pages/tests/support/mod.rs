//! helpers shared by the integration tests: building small pdfs, reading
//! their structure back, and drawing pages to check pixels.

// each test file uses only some of these helpers
#![allow(dead_code)]

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use letters_pdf::{RenderWorker, TileKey};
use lopdf::{Dictionary, Document, Object, ObjectId, Stream};

/// one page of a test document.
#[derive(Clone)]
pub struct PageSpec {
    pub width: f32,
    pub height: f32,
    /// where the paper's bottom left corner is; most files use 0,0
    pub origin: (f32, f32),
    pub rotate: i64,
    /// drawing instructions, in the page's own coordinates
    pub content: String,
}

impl PageSpec {
    pub fn new(width: f32, height: f32) -> Self {
        Self {
            width,
            height,
            origin: (0.0, 0.0),
            rotate: 0,
            content: String::new(),
        }
    }

    pub fn drawing(mut self, content: &str) -> Self {
        self.content = content.to_owned();
        self
    }

    pub fn rotated(mut self, degrees: i64) -> Self {
        self.rotate = degrees;
        self
    }

    pub fn origin(mut self, x: f32, y: f32) -> Self {
        self.origin = (x, y);
        self
    }
}

/// a red square in the page's own coordinates, `size` points wide, with
/// its bottom left corner at x, y.
pub fn red_square(x: f32, y: f32, size: f32) -> String {
    format!("1 0 0 rg {x} {y} {size} {size} re f\n")
}

/// a document with the given pages, all in one flat page tree, each with
/// helvetica available as /F1.
pub fn build(pages: &[PageSpec]) -> Document {
    let mut doc = Document::with_version("1.7");
    let font = doc.add_object(helvetica());
    let root = doc.new_object_id();
    let kids: Vec<Object> = pages
        .iter()
        .map(|spec| Object::Reference(add_page(&mut doc, root, font, spec)))
        .collect();
    let mut tree = Dictionary::new();
    tree.set("Type", Object::Name(b"Pages".to_vec()));
    tree.set("Count", Object::Integer(kids.len() as i64));
    tree.set("Kids", Object::Array(kids));
    doc.objects.insert(root, Object::Dictionary(tree));
    finish_catalog(&mut doc, root);
    doc
}

fn helvetica() -> Dictionary {
    let mut font = Dictionary::new();
    font.set("Type", Object::Name(b"Font".to_vec()));
    font.set("Subtype", Object::Name(b"Type1".to_vec()));
    font.set("BaseFont", Object::Name(b"Helvetica".to_vec()));
    font
}

fn add_page(doc: &mut Document, parent: ObjectId, font: ObjectId, spec: &PageSpec) -> ObjectId {
    let content = doc.add_object(Stream::new(
        Dictionary::new(),
        spec.content.clone().into_bytes(),
    ));
    let (x, y) = spec.origin;
    let mut page = Dictionary::new();
    page.set("Type", Object::Name(b"Page".to_vec()));
    page.set("Parent", Object::Reference(parent));
    page.set("MediaBox", rect(x, y, x + spec.width, y + spec.height));
    page.set("Contents", Object::Reference(content));
    page.set("Resources", font_resources(font));
    if spec.rotate != 0 {
        page.set("Rotate", Object::Integer(spec.rotate));
    }
    doc.add_object(page)
}

pub fn rect(left: f32, bottom: f32, right: f32, top: f32) -> Object {
    Object::Array(vec![left.into(), bottom.into(), right.into(), top.into()])
}

fn font_resources(font: ObjectId) -> Dictionary {
    let mut fonts = Dictionary::new();
    fonts.set("F1", Object::Reference(font));
    let mut resources = Dictionary::new();
    resources.set("Font", Object::Dictionary(fonts));
    resources
}

pub fn finish_catalog(doc: &mut Document, pages_root: ObjectId) {
    let mut catalog = Dictionary::new();
    catalog.set("Type", Object::Name(b"Catalog".to_vec()));
    catalog.set("Pages", Object::Reference(pages_root));
    let catalog = doc.add_object(catalog);
    doc.trailer.set("Root", Object::Reference(catalog));
}

/// the width that tags test page `index`, so the order of pages can be
/// read back from their sizes.
pub fn tag_width(index: usize) -> f32 {
    100.0 + index as f32
}

/// `count` pages, page i being `tag_width(i)` wide and 150 tall, each
/// with one line of text naming it.
pub fn tagged(count: usize) -> Document {
    let pages: Vec<PageSpec> = (0..count)
        .map(|index| {
            PageSpec::new(tag_width(index), 150.0)
                .drawing(&format!("BT /F1 12 Tf 10 10 Td (Page {index}) Tj ET\n"))
        })
        .collect();
    build(&pages)
}

/// the page ids in display order.
pub fn page_ids(doc: &Document) -> Vec<ObjectId> {
    doc.get_pages().into_values().collect()
}

/// a page setting, looking up the page tree when the page doesn't have it.
pub fn inherited(doc: &Document, page: ObjectId, key: &[u8]) -> Option<Object> {
    let mut node = page;
    for _ in 0..64 {
        let dict = doc.get_dictionary(node).ok()?;
        if let Ok(value) = dict.get(key) {
            let (_, value) = doc.dereference(value).ok()?;
            return Some(value.clone());
        }
        node = dict.get(b"Parent").and_then(Object::as_reference).ok()?;
    }
    None
}

/// four numbers of a box setting.
pub fn page_box(doc: &Document, page: ObjectId, key: &[u8]) -> Option<[f32; 4]> {
    let array = inherited(doc, page, key)?;
    let numbers: Vec<f32> = array
        .as_array()
        .ok()?
        .iter()
        .map(|number| number.as_float().unwrap())
        .collect();
    Some([numbers[0], numbers[1], numbers[2], numbers[3]])
}

/// every page's width, in display order.
pub fn widths(doc: &Document) -> Vec<f32> {
    page_ids(doc)
        .into_iter()
        .map(|page| {
            let [left, _, right, _] = page_box(doc, page, b"MediaBox").unwrap();
            right - left
        })
        .collect()
}

/// every page's width as tag indexes, for easy comparison.
pub fn order(doc: &Document) -> Vec<usize> {
    widths(doc)
        .into_iter()
        .map(|width| (width - 100.0).round() as usize)
        .collect()
}

pub fn rotation(doc: &Document, page: ObjectId) -> i64 {
    inherited(doc, page, b"Rotate")
        .and_then(|r| r.as_i64().ok())
        .unwrap_or(0)
}

pub fn save(doc: &mut Document) -> Vec<u8> {
    let mut bytes = Vec::new();
    doc.save_to(&mut bytes).unwrap();
    bytes
}

/// saves and loads again, the way a file on disk would come back.
pub fn reload(doc: &mut Document) -> Document {
    Document::load_mem(&save(doc)).unwrap()
}

/// all of a page's drawing instructions, as text.
pub fn content_text(doc: &Document, page: ObjectId) -> String {
    String::from_utf8_lossy(&doc.get_page_content(page)).into_owned()
}

/// a cheap fingerprint of a document: saved bytes. two documents that
/// save the same are the same for every purpose that matters.
pub fn fingerprint(doc: &Document) -> Vec<u8> {
    let mut copy = doc.clone();
    save(&mut copy)
}

// ---- bookmarks ----------------------------------------------------------

pub struct Bookmark {
    pub title: String,
    pub page: usize,
    pub children: Vec<Bookmark>,
}

pub fn bookmark(title: &str, page: usize, children: Vec<Bookmark>) -> Bookmark {
    Bookmark {
        title: title.to_owned(),
        page,
        children,
    }
}

/// gives a document an outline, every entry leading to a page.
pub fn add_outline(doc: &mut Document, bookmarks: &[Bookmark]) {
    let pages = page_ids(doc);
    let root = doc.new_object_id();
    let (first, last, count) = write_bookmarks(doc, bookmarks, root, &pages);
    let mut dict = Dictionary::new();
    dict.set("Type", Object::Name(b"Outlines".to_vec()));
    dict.set("First", Object::Reference(first));
    dict.set("Last", Object::Reference(last));
    dict.set("Count", Object::Integer(count));
    doc.objects.insert(root, Object::Dictionary(dict));
    doc.catalog_mut()
        .unwrap()
        .set("Outlines", Object::Reference(root));
}

fn write_bookmarks(
    doc: &mut Document,
    bookmarks: &[Bookmark],
    parent: ObjectId,
    pages: &[ObjectId],
) -> (ObjectId, ObjectId, i64) {
    let ids: Vec<ObjectId> = bookmarks.iter().map(|_| doc.new_object_id()).collect();
    let mut count = 0;
    for (index, bookmark) in bookmarks.iter().enumerate() {
        let mut dict = Dictionary::new();
        dict.set("Title", Object::string_literal(bookmark.title.clone()));
        dict.set("Parent", Object::Reference(parent));
        dict.set(
            "Dest",
            Object::Array(vec![
                Object::Reference(pages[bookmark.page]),
                Object::Name(b"Fit".to_vec()),
            ]),
        );
        if index > 0 {
            dict.set("Prev", Object::Reference(ids[index - 1]));
        }
        if index + 1 < ids.len() {
            dict.set("Next", Object::Reference(ids[index + 1]));
        }
        count += 1;
        if !bookmark.children.is_empty() {
            let (first, last, children) =
                write_bookmarks(doc, &bookmark.children, ids[index], pages);
            dict.set("First", Object::Reference(first));
            dict.set("Last", Object::Reference(last));
            dict.set("Count", Object::Integer(children));
            count += children;
        }
        doc.objects.insert(ids[index], Object::Dictionary(dict));
    }
    (ids[0], ids[ids.len() - 1], count)
}

/// the outline flattened: (depth, title, page index it leads to).
pub fn read_outline(doc: &Document) -> Vec<(usize, String, Option<usize>)> {
    let pages = page_ids(doc);
    let mut entries = Vec::new();
    let Ok(root) = doc
        .catalog()
        .and_then(|catalog| catalog.get(b"Outlines"))
        .and_then(Object::as_reference)
    else {
        return entries;
    };
    let first = doc
        .get_dictionary(root)
        .unwrap()
        .get(b"First")
        .and_then(Object::as_reference)
        .ok();
    read_siblings(doc, first, 0, &pages, &mut entries);
    entries
}

fn read_siblings(
    doc: &Document,
    mut next: Option<ObjectId>,
    depth: usize,
    pages: &[ObjectId],
    entries: &mut Vec<(usize, String, Option<usize>)>,
) {
    while let Some(id) = next {
        let dict = doc.get_dictionary(id).unwrap();
        let title = text(dict.get(b"Title").unwrap().as_str().unwrap());
        let page = dict
            .get(b"Dest")
            .ok()
            .and_then(|dest| dest.as_array().ok())
            .and_then(|dest| dest[0].as_reference().ok())
            .and_then(|page| pages.iter().position(|&p| p == page));
        entries.push((depth, title, page));
        let first = dict.get(b"First").and_then(Object::as_reference).ok();
        read_siblings(doc, first, depth + 1, pages, entries);
        next = dict.get(b"Next").and_then(Object::as_reference).ok();
    }
}

/// reads a pdf text string: utf-16 when it starts with a byte order mark.
pub fn text(bytes: &[u8]) -> String {
    match bytes.strip_prefix(&[0xFE, 0xFF]) {
        Some(utf16) => {
            let units: Vec<u16> = utf16
                .as_chunks::<2>()
                .0
                .iter()
                .map(|&pair| u16::from_be_bytes(pair))
                .collect();
            String::from_utf16(&units).unwrap()
        }
        None => String::from_utf8_lossy(bytes).into_owned(),
    }
}

// ---- links and form fields ----------------------------------------------

/// adds a link on page `from` that jumps to page `to`.
pub fn add_link(doc: &mut Document, from: usize, to: usize) -> ObjectId {
    let pages = page_ids(doc);
    let mut link = Dictionary::new();
    link.set("Type", Object::Name(b"Annot".to_vec()));
    link.set("Subtype", Object::Name(b"Link".to_vec()));
    link.set("Rect", rect(0.0, 0.0, 20.0, 20.0));
    link.set(
        "Dest",
        Object::Array(vec![
            Object::Reference(pages[to]),
            Object::Name(b"Fit".to_vec()),
        ]),
    );
    let link = doc.add_object(link);
    add_annotation(doc, pages[from], link);
    link
}

pub fn add_annotation(doc: &mut Document, page: ObjectId, annotation: ObjectId) {
    let dict = doc.get_dictionary_mut(page).unwrap();
    let mut annots = dict
        .get(b"Annots")
        .ok()
        .and_then(|a| a.as_array().ok().cloned())
        .unwrap_or_default();
    annots.push(Object::Reference(annotation));
    dict.set("Annots", Object::Array(annots));
}

/// where each link on a page leads, as page indexes.
pub fn link_targets(doc: &Document, page_index: usize) -> Vec<Option<usize>> {
    let pages = page_ids(doc);
    let page = doc.get_dictionary(pages[page_index]).unwrap();
    let Ok(annots) = page.get(b"Annots") else {
        return Vec::new();
    };
    let (_, annots) = doc.dereference(annots).unwrap();
    annots
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|annot| {
            let (_, annot) = doc.dereference(annot).ok()?;
            let annot = annot.as_dict().ok()?;
            if annot.get(b"Subtype").ok()?.as_name().ok()? != b"Link" {
                return None;
            }
            let target = annot
                .get(b"Dest")
                .ok()
                .and_then(|dest| dest.as_array().ok())
                .and_then(|dest| dest[0].as_reference().ok())
                .and_then(|target| pages.iter().position(|&p| p == target));
            Some(target)
        })
        .collect()
}

/// adds a text field called `name` with one box on each of `pages`.
pub fn add_text_field(doc: &mut Document, name: &str, on_pages: &[usize]) -> ObjectId {
    let pages = page_ids(doc);
    let field = doc.new_object_id();
    let mut kids = Vec::new();
    for &page in on_pages {
        let mut widget = Dictionary::new();
        widget.set("Type", Object::Name(b"Annot".to_vec()));
        widget.set("Subtype", Object::Name(b"Widget".to_vec()));
        widget.set("Rect", rect(10.0, 10.0, 90.0, 30.0));
        widget.set("Parent", Object::Reference(field));
        widget.set("P", Object::Reference(pages[page]));
        let widget = doc.add_object(widget);
        add_annotation(doc, pages[page], widget);
        kids.push(Object::Reference(widget));
    }
    let mut dict = Dictionary::new();
    dict.set("FT", Object::Name(b"Tx".to_vec()));
    dict.set("T", Object::string_literal(name));
    dict.set("Kids", Object::Array(kids));
    doc.objects.insert(field, Object::Dictionary(dict));

    let catalog = doc.catalog_mut().unwrap();
    let mut form = catalog
        .get(b"AcroForm")
        .ok()
        .and_then(|form| form.as_dict().ok().cloned())
        .unwrap_or_default();
    let mut fields = form
        .get(b"Fields")
        .ok()
        .and_then(|fields| fields.as_array().ok().cloned())
        .unwrap_or_default();
    fields.push(Object::Reference(field));
    form.set("Fields", Object::Array(fields));
    form.set("DA", Object::string_literal("/Helv 0 Tf 0 g"));
    catalog.set("AcroForm", Object::Dictionary(form));
    field
}

/// the top level form fields: (name, number of boxes).
pub fn fields(doc: &Document) -> Vec<(String, usize)> {
    let Ok(form) = doc.catalog().and_then(|catalog| catalog.get(b"AcroForm")) else {
        return Vec::new();
    };
    let (_, form) = doc.dereference(form).unwrap();
    let (_, list) = doc
        .dereference(form.as_dict().unwrap().get(b"Fields").unwrap())
        .unwrap();
    list.as_array()
        .unwrap()
        .iter()
        .map(|field| {
            let field = doc.get_dictionary(field.as_reference().unwrap()).unwrap();
            let name =
                String::from_utf8_lossy(field.get(b"T").unwrap().as_str().unwrap()).into_owned();
            let kids = field
                .get(b"Kids")
                .map(|k| k.as_array().unwrap().len())
                .unwrap_or(1);
            (name, kids)
        })
        .collect()
}

/// checks that every reference in the document leads to an object that
/// exists, so nothing was left dangling.
pub fn assert_no_dangling_references(doc: &Document) {
    fn visit(doc: &Document, object: &Object, from: ObjectId) {
        match object {
            Object::Reference(id) => assert!(
                doc.objects.contains_key(id),
                "object {from:?} points at missing object {id:?}"
            ),
            Object::Array(items) => items.iter().for_each(|item| visit(doc, item, from)),
            Object::Dictionary(dict) => dict.iter().for_each(|(_, value)| visit(doc, value, from)),
            Object::Stream(stream) => stream
                .dict
                .iter()
                .for_each(|(_, value)| visit(doc, value, from)),
            _ => {}
        }
    }
    for (&id, object) in &doc.objects {
        visit(doc, object, id);
    }
}

// ---- drawing pages --------------------------------------------------------

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
    WORKER.get_or_init(|| RenderWorker::start(&pdfium_dir(), 8 * 1024 * 1024).unwrap())
}

/// a fresh file path for one test's pdf.
fn temp_file(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join("letters-pages-tests");
    std::fs::create_dir_all(&dir).unwrap();
    dir.join(format!("{name}-{}.pdf", std::process::id()))
}

/// a page drawn at one pixel per point, with its shown size.
pub struct Rendered {
    pub image: Image,
    pub width_pt: f32,
    pub height_pt: f32,
}

/// saves the document and draws one page as pdfium shows it. test pages
/// are kept under 512 points so one tile holds the whole page.
pub fn render(doc: &Document, page: u32, name: &str) -> Rendered {
    let path = temp_file(name);
    let mut copy = doc.clone();
    std::fs::write(&path, save(&mut copy)).unwrap();
    let worker = worker();
    let info = worker.open(&path, None).unwrap();
    let size = info.page_sizes[page as usize];
    assert!(
        size.width_pt <= 512.0 && size.height_pt <= 512.0,
        "test pages must fit one tile"
    );
    let tile = worker
        .tile(TileKey {
            document: info.id,
            page,
            scale_milli: 1000,
            column: 0,
            row: 0,
        })
        .unwrap();
    worker.close(info.id);
    let _ = std::fs::remove_file(&path);
    Rendered {
        image: Image::from_png(&tile),
        width_pt: size.width_pt,
        height_pt: size.height_pt,
    }
}

/// a decoded tile, so tests can look at single pixels.
pub struct Image {
    pub width: u32,
    pub height: u32,
    rgba: Vec<u8>,
}

impl Image {
    pub fn from_png(bytes: &[u8]) -> Self {
        let mut reader = png::Decoder::new(std::io::Cursor::new(bytes))
            .read_info()
            .unwrap();
        let mut rgba = vec![0; reader.output_buffer_size().unwrap()];
        let info = reader.next_frame(&mut rgba).unwrap();
        assert_eq!(info.color_type, png::ColorType::Rgba);
        Self {
            width: info.width,
            height: info.height,
            rgba,
        }
    }

    pub fn pixel(&self, x: u32, y: u32) -> [u8; 3] {
        let start = ((y * self.width + x) * 4) as usize;
        [self.rgba[start], self.rgba[start + 1], self.rgba[start + 2]]
    }

    /// how many pixels in a rectangle (x, y, width, height from the top
    /// left) are not white.
    pub fn inked(&self, x: u32, y: u32, width: u32, height: u32) -> usize {
        let mut count = 0;
        for row in y..(y + height).min(self.height) {
            for column in x..(x + width).min(self.width) {
                if self.pixel(column, row) != WHITE {
                    count += 1;
                }
            }
        }
        count
    }

    /// the smallest rectangle (left, top, right, bottom) holding every
    /// pixel that is not white, or none for a blank page.
    pub fn ink_bounds(&self) -> Option<(u32, u32, u32, u32)> {
        let mut bounds: Option<(u32, u32, u32, u32)> = None;
        for y in 0..self.height {
            for x in 0..self.width {
                if self.pixel(x, y) != WHITE {
                    bounds = Some(match bounds {
                        None => (x, y, x, y),
                        Some((l, t, r, b)) => (l.min(x), t.min(y), r.max(x), b.max(y)),
                    });
                }
            }
        }
        bounds
    }
}

pub const RED: [u8; 3] = [255, 0, 0];
pub const WHITE: [u8; 3] = [255, 255, 255];

/// a png picture of one solid colour, for watermark tests.
pub fn solid_png(width: u32, height: u32, rgba: [u8; 4]) -> Vec<u8> {
    let mut bytes = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut bytes, width, height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header().unwrap();
        let pixels: Vec<u8> = (0..width * height).flat_map(|_| rgba).collect();
        writer.write_image_data(&pixels).unwrap();
    }
    bytes
}

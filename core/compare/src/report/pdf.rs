//! writing the comparison report as a pdf, with lopdf.
//!
//! the report is laid out top to bottom on us letter pages: a title page
//! with the summary table, then a section per page pair with thumbnails of
//! both pages (changed areas outlined, red on the old page and green on the
//! new) and the changes as short sentences grouped by category.
//!
//! it is made to be read by everyone:
//! - text uses Helvetica, so it can be selected, searched and copied
//! - the document has a title (shown in the window), a language and
//!   bookmarks for the summary and every page pair
//! - it is a tagged pdf: headings, paragraphs, the table, lists and
//!   pictures are marked in a structure tree in reading order, pictures
//!   have a text alternative, and decorations (lines, outlines, page
//!   numbers) are marked as artifacts so screen readers skip them

use lopdf::content::{Content, Operation};
use lopdf::{
    dictionary, text_string, Dictionary, Document, Object, ObjectId, Stream, StringFormat,
};

use super::winansi;
use super::{
    change_sentence, sections, summary_rows, summary_sentences, version_lines, ReportOptions,
    Section,
};
use crate::geometry::Rect;
use crate::result::{Category, ComparisonResult, PagePair};
use crate::source::{PageSource, RgbaImage};
use crate::standard_fonts::{StandardFont, HELVETICA, HELVETICA_BOLD};
use crate::CompareError;

const PAGE_WIDTH: f32 = 612.0;
const PAGE_HEIGHT: f32 = 792.0;
const MARGIN: f32 = 54.0;
/// room kept at the bottom of every page for its page number
const FOOTER_ROOM: f32 = 24.0;
const TEXT_WIDTH: f32 = PAGE_WIDTH - 2.0 * MARGIN;
const BODY_SIZE: f32 = 10.0;
/// line height as a multiple of the font size
const LEADING: f32 = 1.35;

/// the largest a thumbnail is drawn, in points
const THUMB_WIDTH: f32 = 240.0;
const THUMB_HEIGHT: f32 = 300.0;
const THUMB_GAP: f32 = 24.0;
/// thumbnails are drawn with this many pixels per point of the report, so
/// they stay sharp when zoomed a little
const THUMB_SHARPNESS: f32 = 2.0;

const OLD_MARK: [f32; 3] = [0.85, 0.1, 0.1];
const NEW_MARK: [f32; 3] = [0.1, 0.6, 0.2];
const QUIET_TEXT: [f32; 3] = [0.35, 0.35, 0.35];

#[derive(Clone, Copy, Debug, PartialEq)]
enum Font {
    Regular,
    Bold,
}

impl Font {
    fn resource_name(&self) -> &'static str {
        match self {
            Font::Regular => "F1",
            Font::Bold => "F2",
        }
    }

    fn metrics(&self) -> &'static StandardFont {
        match self {
            Font::Regular => &HELVETICA,
            Font::Bold => &HELVETICA_BOLD,
        }
    }
}

/// one element of the structure tree.
struct Element {
    role: &'static str,
    parent: usize,
    kids: Vec<Kid>,
    alt: Option<String>,
}

enum Kid {
    Element(usize),
    /// a piece of marked content on a page
    Marked {
        page: usize,
        mcid: i64,
    },
}

/// a page being written.
struct ReportPage {
    id: ObjectId,
    operations: Vec<Operation>,
    images: Vec<(String, ObjectId)>,
    /// for each marked content id on the page, the element it belongs to
    marked: Vec<usize>,
}

/// lays out the report and builds the pdf objects.
struct Writer {
    document: Document,
    pages: Vec<ReportPage>,
    elements: Vec<Element>,
    /// (title, page, distance from the top) of each bookmark
    bookmarks: Vec<(String, usize, f32)>,
    /// where the next thing goes, measured from the top of the current page
    y: f32,
    images: usize,
}

/// the element every other one belongs to.
const DOCUMENT: usize = 0;

/// writes the report as pdf bytes. thumbnails are drawn with the sources'
/// renderers, one page at a time; when a page can't be drawn, its
/// thumbnail says so and the report is still written.
pub fn report_pdf(
    result: &ComparisonResult,
    left: &dyn PageSource,
    right: &dyn PageSource,
    options: &ReportOptions,
) -> Result<Vec<u8>, CompareError> {
    let mut writer = Writer::new();
    writer.title_page(result, options);
    let sections = sections(result);
    for (index, section) in sections.iter().enumerate() {
        writer.section(section, index == 0, left, right, options);
    }
    writer.finish(options)
}

impl Writer {
    fn new() -> Writer {
        let mut writer = Writer {
            document: Document::with_version("1.7"),
            pages: Vec::new(),
            elements: vec![Element {
                role: "Document",
                parent: DOCUMENT,
                kids: Vec::new(),
                alt: None,
            }],
            bookmarks: Vec::new(),
            y: MARGIN,
            images: 0,
        };
        writer.new_page();
        writer
    }

    fn new_page(&mut self) {
        let id = self.document.new_object_id();
        self.pages.push(ReportPage {
            id,
            operations: Vec::new(),
            images: Vec::new(),
            marked: Vec::new(),
        });
        self.y = MARGIN;
    }

    fn page(&mut self) -> &mut ReportPage {
        self.pages.last_mut().expect("there is always a page")
    }

    fn room_left(&self) -> f32 {
        PAGE_HEIGHT - MARGIN - FOOTER_ROOM - self.y
    }

    fn ensure_room(&mut self, height: f32) {
        if self.room_left() < height {
            self.new_page();
        }
    }

    fn add_element(&mut self, role: &'static str, parent: usize) -> usize {
        let index = self.elements.len();
        self.elements.push(Element {
            role,
            parent,
            kids: Vec::new(),
            alt: None,
        });
        self.elements[parent].kids.push(Kid::Element(index));
        index
    }

    /// starts content that belongs to `element`.
    fn begin_marked(&mut self, element: usize) {
        let page_index = self.pages.len() - 1;
        let role = self.elements[element].role;
        let page = self.page();
        let mcid = page.marked.len() as i64;
        page.marked.push(element);
        page.operations.push(Operation::new(
            "BDC",
            vec![
                Object::from(role),
                Object::Dictionary(dictionary! { "MCID" => mcid }),
            ],
        ));
        self.elements[element].kids.push(Kid::Marked {
            page: page_index,
            mcid,
        });
    }

    fn end_marked(&mut self) {
        self.page().operations.push(Operation::new("EMC", vec![]));
    }

    /// draws decoration that carries no meaning, marked so readers skip it.
    fn artifact(&mut self, operations: Vec<Operation>) {
        let page = self.page();
        page.operations
            .push(Operation::new("BMC", vec![Object::from("Artifact")]));
        page.operations.extend(operations);
        page.operations.push(Operation::new("EMC", vec![]));
    }

    fn bookmark(&mut self, title: &str) {
        let page = self.pages.len() - 1;
        self.bookmarks.push((title.to_owned(), page, self.y));
    }

    /// wrapped text as one element, continuing onto new pages as needed.
    fn paragraph(
        &mut self,
        role: &'static str,
        parent: usize,
        text: &str,
        style: TextLook,
    ) -> usize {
        let element = self.add_element(role, parent);
        self.text_into(element, text, style);
        element
    }

    /// wrapped text as content of an existing element.
    fn text_into(&mut self, element: usize, text: &str, style: TextLook) {
        let line_height = style.size * LEADING;
        let mut open = false;
        for line in wrap(text, style.font, style.size, TEXT_WIDTH - style.indent) {
            if self.room_left() < line_height {
                if open {
                    self.end_marked();
                    open = false;
                }
                self.new_page();
            }
            if !open {
                self.begin_marked(element);
                open = true;
            }
            let baseline = self.y + style.size;
            let operations = text_operations(MARGIN + style.indent, baseline, style, &line);
            self.page().operations.extend(operations);
            self.y += line_height;
        }
        if open {
            self.end_marked();
        }
    }

    fn heading(
        &mut self,
        role: &'static str,
        text: &str,
        size: f32,
        space_before: f32,
        keep_with: f32,
    ) {
        if self.y > MARGIN {
            self.y += space_before;
        }
        // a heading never sits alone at the bottom of a page
        self.ensure_room(size * LEADING + keep_with);
        if role != "H3" {
            self.bookmark(text);
        }
        self.paragraph(role, DOCUMENT, text, TextLook::bold(size));
        self.y += size * 0.3;
    }

    fn title_page(&mut self, result: &ComparisonResult, options: &ReportOptions) {
        self.heading("H1", &options.title, 22.0, 0.0, 60.0);
        for line in version_lines(options) {
            self.paragraph("P", DOCUMENT, &line, TextLook::body());
        }
        self.heading("H2", "Summary", 14.0, 14.0, 120.0);
        self.summary_table(&summary_rows(result));
        self.y += 10.0;
        for sentence in summary_sentences(result) {
            self.paragraph("P", DOCUMENT, &sentence, TextLook::body());
            self.y += 3.0;
        }
    }

    /// the counts by category as a real table, with header cells.
    fn summary_table(&mut self, rows: &[(String, u32)]) {
        let row_height = 18.0;
        let columns = [MARGIN, MARGIN + 240.0, MARGIN + 340.0];
        self.ensure_room(row_height * (rows.len() + 1) as f32);
        let table = self.add_element("Table", DOCUMENT);
        let header = [
            ("What changed".to_owned(), true),
            ("Changes".to_owned(), true),
        ];
        let mut all_rows = vec![header.to_vec()];
        for (index, (label, count)) in rows.iter().enumerate() {
            let is_total = index + 1 == rows.len();
            all_rows.push(vec![
                (label.clone(), is_total),
                (count.to_string(), is_total),
            ]);
        }
        for (row_number, cells) in all_rows.iter().enumerate() {
            let row = self.add_element("TR", table);
            for (column, (text, bold)) in cells.iter().enumerate() {
                let role = if row_number == 0 || column == 0 {
                    "TH"
                } else {
                    "TD"
                };
                let cell = self.add_element(role, row);
                let look = if *bold {
                    TextLook::bold(BODY_SIZE)
                } else {
                    TextLook::body()
                };
                self.begin_marked(cell);
                let operations = text_operations(columns[column] + 4.0, self.y + 12.0, look, text);
                self.page().operations.extend(operations);
                self.end_marked();
            }
            // a thin line under every row
            let line_y = PAGE_HEIGHT - (self.y + row_height);
            self.artifact(vec![
                Operation::new("RG", vec![0.75.into(), 0.75.into(), 0.75.into()]),
                Operation::new("w", vec![if row_number == 0 { 1.0 } else { 0.5 }.into()]),
                Operation::new("m", vec![columns[0].into(), line_y.into()]),
                Operation::new("l", vec![columns[2].into(), line_y.into()]),
                Operation::new("S", vec![]),
            ]);
            self.y += row_height;
        }
    }

    fn section(
        &mut self,
        section: &Section,
        first: bool,
        left: &dyn PageSource,
        right: &dyn PageSource,
        options: &ReportOptions,
    ) {
        let thumbnails = options.thumbnails && (left.can_render() || right.can_render());
        let thumb_room = if thumbnails {
            THUMB_HEIGHT + 40.0
        } else {
            40.0
        };
        if first || self.room_left() < thumb_room + 40.0 {
            self.new_page();
        }
        self.heading("H2", &section.heading, 14.0, 18.0, thumb_room);
        if thumbnails {
            self.thumbnails(section.pair, left, right);
        }
        for note in &section.pair.notes {
            self.paragraph("P", DOCUMENT, note, TextLook::quiet());
            self.y += 3.0;
        }
        for (category, changes) in &section.groups {
            self.heading("H3", category.label(), 11.0, 8.0, BODY_SIZE * LEADING * 2.0);
            let list = self.add_element("L", DOCUMENT);
            for change in changes {
                let item = self.add_element("LI", list);
                let body = self.add_element("LBody", item);
                let sentence = format!("•  {}", change_sentence(change));
                self.text_into(body, &sentence, TextLook::body().indented(8.0));
                self.y += 2.0;
            }
        }
    }

    /// pictures of both pages side by side, changed areas outlined.
    fn thumbnails(&mut self, pair: &PagePair, left: &dyn PageSource, right: &dyn PageSource) {
        let sides = [
            (pair.left_page, pair.left_size, left, "Old", OLD_MARK),
            (pair.right_page, pair.right_size, right, "New", NEW_MARK),
        ];
        let top = self.y;
        let mut lowest = top;
        for (column, (page, size, source, label, colour)) in sides.into_iter().enumerate() {
            let x = MARGIN + column as f32 * (THUMB_WIDTH + THUMB_GAP);
            let (Some(page), Some(size)) = (page, size) else {
                let bottom = self.missing_page_box(x, top, label);
                lowest = lowest.max(bottom);
                continue;
            };
            let scale = (THUMB_WIDTH / size.width).min(THUMB_HEIGHT / size.height);
            let (width, height) = (size.width * scale, size.height * scale);
            let marks = marked_rects(pair, column == 0);
            let alt = format!(
                "{label} page {}, with {} changed {} outlined",
                page + 1,
                marks.len(),
                if marks.len() == 1 { "area" } else { "areas" }
            );
            let picture = source.render(page, scale * THUMB_SHARPNESS).ok();
            self.figure(x, top, width, height, picture.as_ref(), &alt);
            self.outline_marks(x, top, scale, &marks, colour);
            let caption_y = top + height + 4.0;
            let caption = format!("{label} page {}", page + 1);
            let element = self.add_element("Caption", DOCUMENT);
            self.begin_marked(element);
            let operations = text_operations(x, caption_y + 9.0, TextLook::quiet(), &caption);
            self.page().operations.extend(operations);
            self.end_marked();
            lowest = lowest.max(caption_y + 14.0);
        }
        self.y = lowest + 8.0;
    }

    /// a page picture as a tagged figure with a text alternative.
    fn figure(
        &mut self,
        x: f32,
        top: f32,
        width: f32,
        height: f32,
        picture: Option<&RgbaImage>,
        alt: &str,
    ) {
        let element = self.add_element("Figure", DOCUMENT);
        self.elements[element].alt = Some(alt.to_owned());
        let bottom = PAGE_HEIGHT - top - height;
        self.begin_marked(element);
        match picture {
            Some(picture) => {
                let name = format!("Im{}", self.images);
                self.images += 1;
                let id = self.document.add_object(image_stream(picture));
                self.page().images.push((name.clone(), id));
                self.page().operations.extend([
                    Operation::new("q", vec![]),
                    Operation::new(
                        "cm",
                        vec![
                            width.into(),
                            0.into(),
                            0.into(),
                            height.into(),
                            x.into(),
                            bottom.into(),
                        ],
                    ),
                    Operation::new("Do", vec![Object::Name(name.into_bytes())]),
                    Operation::new("Q", vec![]),
                ]);
            }
            None => {
                let operations = text_operations(
                    x + 8.0,
                    top + 20.0,
                    TextLook::quiet(),
                    "Picture not available",
                );
                self.page().operations.extend(operations);
            }
        }
        self.end_marked();
        // a hairline frame shows where the page ends
        self.artifact(rectangle_operations(
            &Rect::new(x, top, width, height),
            [0.7, 0.7, 0.7],
            0.5,
        ));
    }

    fn missing_page_box(&mut self, x: f32, top: f32, label: &str) -> f32 {
        let height = 60.0;
        self.artifact(rectangle_operations(
            &Rect::new(x, top, THUMB_WIDTH, height),
            [0.8, 0.8, 0.8],
            0.5,
        ));
        let element = self.add_element("P", DOCUMENT);
        self.begin_marked(element);
        let text = format!("No {} page", label.to_lowercase());
        let operations = text_operations(x + 8.0, top + 34.0, TextLook::quiet(), &text);
        self.page().operations.extend(operations);
        self.end_marked();
        top + height + 8.0
    }

    fn outline_marks(&mut self, x: f32, top: f32, scale: f32, marks: &[Rect], colour: [f32; 3]) {
        let mut operations = Vec::new();
        for mark in marks {
            // a caret (zero width) still needs to be visible
            let width = (mark.width * scale).max(1.5);
            let height = (mark.height * scale).max(1.5);
            let rect = Rect::new(
                x + mark.x * scale - 1.0,
                top + mark.y * scale - 1.0,
                width + 2.0,
                height + 2.0,
            );
            operations.extend(rectangle_operations(&rect, colour, 1.0));
        }
        if !operations.is_empty() {
            self.artifact(operations);
        }
    }

    /// builds every pdf object and returns the file's bytes.
    fn finish(mut self, options: &ReportOptions) -> Result<Vec<u8>, CompareError> {
        let pages_id = self.document.new_object_id();
        let regular = self.document.add_object(font_dictionary("Helvetica"));
        let bold = self.document.add_object(font_dictionary("Helvetica-Bold"));
        let tree_root = self.document.new_object_id();
        let element_ids: Vec<ObjectId> = self
            .elements
            .iter()
            .map(|_| self.document.new_object_id())
            .collect();
        let page_count = self.pages.len();
        let page_ids: Vec<ObjectId> = self.pages.iter().map(|page| page.id).collect();

        let mut parent_numbers = Vec::new();
        for (index, page) in std::mem::take(&mut self.pages).into_iter().enumerate() {
            let mut operations = page.operations;
            operations.push(Operation::new("BMC", vec![Object::from("Artifact")]));
            let number = format!("Page {} of {}", index + 1, page_count);
            let width = HELVETICA.text_width(&number, 9.0);
            operations.extend(text_operations(
                (PAGE_WIDTH - width) / 2.0,
                PAGE_HEIGHT - MARGIN + 6.0,
                TextLook::quiet().sized(9.0),
                &number,
            ));
            operations.push(Operation::new("EMC", vec![]));
            let content = Content { operations }
                .encode()
                .map_err(|error| CompareError::Report(error.to_string()))?;
            let mut stream = Stream::new(Dictionary::new(), content);
            let _ = stream.compress();
            let content_id = self.document.add_object(stream);
            let mut pictures = Dictionary::new();
            for (name, id) in &page.images {
                pictures.set(name.as_bytes().to_vec(), Object::Reference(*id));
            }
            let page_dictionary = dictionary! {
                "Type" => "Page",
                "Parent" => pages_id,
                "MediaBox" => vec![0.into(), 0.into(), PAGE_WIDTH.into(), PAGE_HEIGHT.into()],
                "Contents" => content_id,
                "Resources" => dictionary! {
                    "Font" => dictionary! { "F1" => regular, "F2" => bold },
                    "XObject" => pictures,
                },
                "StructParents" => index as i64,
                // the tab order follows the structure tree
                "Tabs" => "S",
            };
            self.document
                .objects
                .insert(page.id, Object::Dictionary(page_dictionary));
            let owners: Vec<Object> = page
                .marked
                .iter()
                .map(|&element| element_ids[element].into())
                .collect();
            parent_numbers.push((index as i64).into());
            parent_numbers.push(Object::Array(owners));
        }
        self.document.objects.insert(
            pages_id,
            Object::Dictionary(dictionary! {
                "Type" => "Pages",
                "Kids" => page_ids.iter().map(|&id| Object::Reference(id)).collect::<Vec<_>>(),
                "Count" => page_count as i64,
            }),
        );
        self.write_structure(
            &element_ids,
            &page_ids,
            tree_root,
            parent_numbers,
            page_count,
        );
        let outline = self.write_outline(&page_ids);
        self.write_catalog(pages_id, tree_root, outline, options);

        let mut bytes = Vec::new();
        self.document
            .save_to(&mut bytes)
            .map_err(|error| CompareError::Report(error.to_string()))?;
        Ok(bytes)
    }

    fn write_structure(
        &mut self,
        element_ids: &[ObjectId],
        page_ids: &[ObjectId],
        tree_root: ObjectId,
        parent_numbers: Vec<Object>,
        page_count: usize,
    ) {
        for (index, element) in self.elements.iter().enumerate() {
            let kids: Vec<Object> = element
                .kids
                .iter()
                .map(|kid| match kid {
                    Kid::Element(child) => Object::Reference(element_ids[*child]),
                    Kid::Marked { page, mcid } => Object::Dictionary(dictionary! {
                        "Type" => "MCR",
                        "Pg" => page_ids[*page],
                        "MCID" => *mcid,
                    }),
                })
                .collect();
            let parent = if index == DOCUMENT {
                tree_root
            } else {
                element_ids[element.parent]
            };
            let mut dict = dictionary! {
                "Type" => "StructElem",
                "S" => element.role,
                "P" => parent,
                "K" => kids,
            };
            if let Some(alt) = &element.alt {
                dict.set("Alt", text_string(alt));
            }
            self.document
                .objects
                .insert(element_ids[index], Object::Dictionary(dict));
        }
        self.document.objects.insert(
            tree_root,
            Object::Dictionary(dictionary! {
                "Type" => "StructTreeRoot",
                "K" => element_ids[DOCUMENT],
                "ParentTree" => dictionary! { "Nums" => parent_numbers },
                "ParentTreeNextKey" => page_count as i64,
            }),
        );
    }

    /// bookmarks for the summary and every page pair, in order.
    fn write_outline(&mut self, page_ids: &[ObjectId]) -> Option<ObjectId> {
        if self.bookmarks.is_empty() {
            return None;
        }
        let outline_id = self.document.new_object_id();
        let ids: Vec<ObjectId> = self
            .bookmarks
            .iter()
            .map(|_| self.document.new_object_id())
            .collect();
        for (index, (title, page, y)) in self.bookmarks.iter().enumerate() {
            let mut item = dictionary! {
                "Title" => text_string(title),
                "Parent" => outline_id,
                "Dest" => vec![
                    page_ids[*page].into(),
                    "XYZ".into(),
                    0.into(),
                    (PAGE_HEIGHT - y + 4.0).into(),
                    Object::Null,
                ],
            };
            if index > 0 {
                item.set("Prev", ids[index - 1]);
            }
            if index + 1 < ids.len() {
                item.set("Next", ids[index + 1]);
            }
            self.document
                .objects
                .insert(ids[index], Object::Dictionary(item));
        }
        self.document.objects.insert(
            outline_id,
            Object::Dictionary(dictionary! {
                "Type" => "Outlines",
                "First" => ids[0],
                "Last" => ids[ids.len() - 1],
                "Count" => ids.len() as i64,
            }),
        );
        Some(outline_id)
    }

    fn write_catalog(
        &mut self,
        pages_id: ObjectId,
        tree_root: ObjectId,
        outline: Option<ObjectId>,
        options: &ReportOptions,
    ) {
        let mut catalog = dictionary! {
            "Type" => "Catalog",
            "Pages" => pages_id,
            "StructTreeRoot" => tree_root,
            "MarkInfo" => dictionary! { "Marked" => true },
            "Lang" => Object::string_literal(options.language.clone()),
            // show the title, not the file name, in the reader's window
            "ViewerPreferences" => dictionary! { "DisplayDocTitle" => true },
        };
        if let Some(outline) = outline {
            catalog.set("Outlines", outline);
            catalog.set("PageMode", "UseOutlines");
        }
        let catalog_id = self.document.add_object(catalog);
        let info_id = self.document.add_object(dictionary! {
            "Title" => text_string(&options.title),
            "Subject" => text_string(&format!("{} compared with {}", options.old_name, options.new_name)),
            "Creator" => text_string("Letters"),
            "Producer" => text_string("Letters"),
        });
        self.document.trailer.set("Root", catalog_id);
        self.document.trailer.set("Info", info_id);
    }
}

/// the rectangles to outline on one side's thumbnail: every change on that
/// page except page-wide ones, which would only outline the whole page.
fn marked_rects(pair: &PagePair, old_side: bool) -> Vec<Rect> {
    pair.changes
        .iter()
        .filter(|change| change.category != Category::Page)
        .filter(|change| !matches!(change.kind, crate::result::ChangeKind::Resized))
        .flat_map(|change| {
            let (page, rects) = if old_side {
                (change.left_page, &change.left_rects)
            } else {
                (change.right_page, &change.right_rects)
            };
            let wanted = if old_side {
                pair.left_page
            } else {
                pair.right_page
            };
            if page == wanted {
                rects.clone()
            } else {
                Vec::new()
            }
        })
        .collect()
}

/// how a piece of text looks.
#[derive(Clone, Copy, Debug)]
struct TextLook {
    font: Font,
    size: f32,
    colour: [f32; 3],
    indent: f32,
}

impl TextLook {
    fn body() -> Self {
        Self {
            font: Font::Regular,
            size: BODY_SIZE,
            colour: [0.0, 0.0, 0.0],
            indent: 0.0,
        }
    }

    fn bold(size: f32) -> Self {
        Self {
            font: Font::Bold,
            size,
            ..Self::body()
        }
    }

    fn quiet() -> Self {
        Self {
            colour: QUIET_TEXT,
            size: 9.0,
            ..Self::body()
        }
    }

    fn sized(self, size: f32) -> Self {
        Self { size, ..self }
    }

    fn indented(self, indent: f32) -> Self {
        Self { indent, ..self }
    }
}

/// draws one line of text with its baseline `baseline` points from the top.
fn text_operations(x: f32, baseline: f32, look: TextLook, text: &str) -> Vec<Operation> {
    vec![
        Operation::new("BT", vec![]),
        Operation::new(
            "rg",
            vec![
                look.colour[0].into(),
                look.colour[1].into(),
                look.colour[2].into(),
            ],
        ),
        Operation::new(
            "Tf",
            vec![look.font.resource_name().into(), look.size.into()],
        ),
        Operation::new("Td", vec![x.into(), (PAGE_HEIGHT - baseline).into()]),
        Operation::new(
            "Tj",
            vec![Object::String(winansi::encode(text), StringFormat::Literal)],
        ),
        Operation::new("ET", vec![]),
    ]
}

/// strokes a rectangle given from the top left of the page.
fn rectangle_operations(rect: &Rect, colour: [f32; 3], width: f32) -> Vec<Operation> {
    vec![
        Operation::new(
            "RG",
            vec![colour[0].into(), colour[1].into(), colour[2].into()],
        ),
        Operation::new("w", vec![width.into()]),
        Operation::new(
            "re",
            vec![
                rect.x.into(),
                (PAGE_HEIGHT - rect.bottom()).into(),
                rect.width.into(),
                rect.height.into(),
            ],
        ),
        Operation::new("S", vec![]),
    ]
}

/// breaks text into lines no wider than `width` points.
fn wrap(text: &str, font: Font, size: f32, width: f32) -> Vec<String> {
    let metrics = font.metrics();
    let mut lines = Vec::new();
    let mut line = String::new();
    for word in text.split(' ').filter(|word| !word.is_empty()) {
        let candidate = if line.is_empty() {
            word.to_owned()
        } else {
            format!("{line} {word}")
        };
        if metrics.text_width(&candidate, size) <= width {
            line = candidate;
            continue;
        }
        if !line.is_empty() {
            lines.push(std::mem::take(&mut line));
        }
        // a word wider than the line is broken wherever it must be
        let mut piece = String::new();
        for ch in word.chars() {
            piece.push(ch);
            if metrics.text_width(&piece, size) > width && piece.chars().count() > 1 {
                piece.pop();
                lines.push(std::mem::take(&mut piece));
                piece.push(ch);
            }
        }
        line = piece;
    }
    if !line.is_empty() || lines.is_empty() {
        lines.push(line);
    }
    lines
}

fn font_dictionary(name: &str) -> Dictionary {
    dictionary! {
        "Type" => "Font",
        "Subtype" => "Type1",
        "BaseFont" => name,
        "Encoding" => "WinAnsiEncoding",
    }
}

/// a page picture as a compressed rgb image, laid on white paper.
fn image_stream(picture: &RgbaImage) -> Stream {
    let mut rgb = Vec::with_capacity(picture.width as usize * picture.height as usize * 3);
    for y in 0..picture.height {
        for x in 0..picture.width {
            rgb.extend_from_slice(&picture.rgb_on_white(x, y));
        }
    }
    let mut stream = Stream::new(
        dictionary! {
            "Type" => "XObject",
            "Subtype" => "Image",
            "Width" => picture.width as i64,
            "Height" => picture.height as i64,
            "ColorSpace" => "DeviceRGB",
            "BitsPerComponent" => 8,
        },
        rgb,
    );
    let _ = stream.compress();
    stream
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wrapping_keeps_lines_inside_the_width() {
        let text = "the quick brown fox jumps over the lazy dog ".repeat(10);
        let lines = wrap(&text, Font::Regular, 10.0, 200.0);
        assert!(lines.len() > 5);
        for line in &lines {
            assert!(HELVETICA.text_width(line, 10.0) <= 200.0, "{line}");
        }
    }

    #[test]
    fn long_words_are_broken() {
        let lines = wrap(&"x".repeat(200), Font::Regular, 10.0, 100.0);
        assert!(lines.len() > 1);
        assert!(lines
            .iter()
            .all(|line| HELVETICA.text_width(line, 10.0) <= 100.0));
    }
}

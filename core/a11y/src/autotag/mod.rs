//! builds tags for a document that has none.
//!
//! this is a best guess from the layout, and says so: every decision it
//! was unsure about is returned for the wizard to show. the steps:
//! 1. read each page's content (`content`) and work out its layout
//!    (`analyse`): lines, paragraphs, pictures, ruled tables (`grid`),
//!    running headers, footers and page numbers
//! 2. classify paragraphs across the whole document: the most common text
//!    size is body text; bigger or bold short paragraphs are headings,
//!    ranked by size into levels; paragraphs starting with a bullet or
//!    number are list items
//! 3. plan the tree: Document › H1…H6, P, L › LI › Lbl/LBody, Figure,
//!    Table › TR › TH/TD, with headers, footers, page numbers, lines and
//!    backgrounds as artifacts
//! 4. wrap each piece of content in marked content with an mcid
//!    (`rewrite`), then write the structure tree, the parent tree, the
//!    MarkInfo flag and the role map (`tagwriter`)
//! 5. tag links, comments and form fields and make tab order follow the tags

mod analyse;
mod grid;

use std::collections::{BTreeMap, HashMap, HashSet};

use lopdf::{Document, Object, ObjectId};
use serde::{Deserialize, Serialize};

use crate::content::{read_page, Mark, PageContent};
use crate::error::A11yError;
use crate::geometry::{PageFrame, Rect};
use crate::objects::{dict_mut, ensure_editable, name, page_ids, ObjectRef};
use crate::rewrite::{replace_content, wrap_units, ArtifactKind, Unit, Wrap};
use crate::structure::StructTree;
use crate::tagwriter::TagWriter;

use analyse::{analyse_page, group_lines, margin_band, margin_key, Item, Line, PageLayout};

/// settings for `auto_tag`.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct AutoTagOptions {
    /// throw away existing tags and build new ones
    pub replace_existing: bool,
    /// the document's language, set at the same time when given
    pub language: Option<String>,
}

/// how sure the tagger was about a decision.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Confidence {
    /// probably right, but worth a look (a figure that needs alt text)
    Medium,
    /// a guess: check it (a table's header row, a heading level)
    Low,
}

/// one decision the wizard should show the person.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TagDecision {
    pub page: u32,
    pub rect: Option<Rect>,
    /// the tag the decision made, once written
    pub element: Option<ObjectRef>,
    pub decision: String,
    pub confidence: Confidence,
}

/// how many of each kind of tag were made.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TagCounts {
    pub headings: usize,
    pub paragraphs: usize,
    pub lists: usize,
    pub figures: usize,
    pub tables: usize,
    pub artifacts: usize,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AutoTagResult {
    pub changes: Vec<String>,
    pub decisions: Vec<TagDecision>,
    pub counts: TagCounts,
}

/// builds tags for the whole document from its layout. refuses a tagged
/// document unless `replace_existing` is set.
pub fn auto_tag(doc: &mut Document, options: &AutoTagOptions) -> Result<AutoTagResult, A11yError> {
    ensure_editable(doc)?;
    if let Some(language) = &options.language {
        if !crate::rules::is_valid_language_tag(language.trim()) {
            return Err(A11yError::InvalidLanguage(language.clone()));
        }
    }
    let tagged = StructTree::read(doc).is_some_and(|tree| !tree.elements.is_empty());
    if tagged && !options.replace_existing {
        return Err(A11yError::AlreadyTagged);
    }
    let pages: Vec<u32> = (0..page_ids(doc).len() as u32).collect();
    // existing marks are thrown away, except decoration, which stays decoration
    let eligible = |mark: Mark| mark != Mark::Artifact;
    let plan = build_plan(doc, &pages, &eligible)?;
    remove_old_tree(doc)?;
    let mut result = write_plan(doc, plan, Mode::Create)?;
    let annotations = crate::fixes::tag_annotations(doc)?;
    result.changes.extend(
        annotations
            .changes
            .into_iter()
            .filter(|c| !c.starts_with("Every")),
    );
    if has_annotations(doc) {
        result
            .changes
            .extend(crate::fixes::set_tab_order(doc)?.changes);
    }
    if let Some(language) = &options.language {
        result
            .changes
            .extend(crate::fixes::set_language(doc, language)?.changes);
    }
    result.changes.push("These tags are a best guess from the layout. Review the decisions listed, especially headings, tables and the reading order.".into());
    Ok(result)
}

/// tags the untagged content of one page of a tagged document, adding the
/// new tags at the end of the tags. tagged content is left as it is.
pub fn tag_untagged_content(doc: &mut Document, page: u32) -> Result<AutoTagResult, A11yError> {
    ensure_editable(doc)?;
    if !StructTree::read(doc).is_some_and(|tree| !tree.elements.is_empty()) {
        return Err(A11yError::NotTagged);
    }
    crate::fixes::page_id(doc, page)?;
    let eligible = |mark: Mark| mark == Mark::Untagged;
    let plan = build_plan(doc, &[page], &eligible)?;
    let mut result = write_plan(doc, plan, Mode::Append)?;
    result.changes.push(
        "The new tags are at the end of the tags; check their place in the reading order.".into(),
    );
    Ok(result)
}

fn has_annotations(doc: &Document) -> bool {
    page_ids(doc).into_iter().any(|page| {
        doc.get_dictionary(page)
            .ok()
            .and_then(|dict| crate::objects::get_array(doc, dict, b"Annots"))
            .is_some_and(|list| !list.is_empty())
    })
}

// ---- planning ---------------------------------------------------------------------

/// a structure element to be written. leaves own content; the others
/// only hold leaves.
struct Node {
    role: String,
    parent: Option<usize>,
    page: u32,
    scope: Option<&'static str>,
}

/// one page's content and what to do with each unit of it.
struct PagePlan {
    page: u32,
    content: PageContent,
    wraps: BTreeMap<(usize, usize), Wrap>,
}

struct Plan {
    nodes: Vec<Node>,
    pages: Vec<PagePlan>,
    /// decisions, with the node they are about
    decisions: Vec<(TagDecision, Option<usize>)>,
    counts: TagCounts,
}

impl Plan {
    fn node(&mut self, role: &str, parent: Option<usize>, page: u32) -> usize {
        self.nodes.push(Node {
            role: role.to_owned(),
            parent,
            page,
            scope: None,
        });
        self.nodes.len() - 1
    }

    fn decide(
        &mut self,
        page: u32,
        rect: Option<Rect>,
        node: Option<usize>,
        confidence: Confidence,
        decision: String,
    ) {
        self.decisions.push((
            TagDecision {
                page,
                rect,
                element: None,
                decision,
                confidence,
            },
            node,
        ));
    }
}

/// the operator range of a text run, picture or shape: the unit that is
/// wrapped as a whole.
fn text_unit(content: &PageContent, run: usize) -> (usize, usize) {
    let index = content.texts[run].op_index;
    (index, index)
}

fn image_unit(content: &PageContent, image: usize) -> (usize, usize) {
    let index = content.images[image].op_index;
    (index, index)
}

/// the sizes text appears at, rounded to half points, so 11.98 and 12 match.
fn size_key(size: f32) -> i64 {
    (size * 2.0).round() as i64
}

/// what body text looks like across the document: the size (and
/// boldness) with the most characters.
struct BodyStyle {
    size: f32,
    bold: bool,
}

/// table cells count too: a page that is mostly a table with a title
/// above it still has the title bigger than its "body" text.
fn body_style(
    layouts: &[(u32, PageLayout)],
    contents: &[(u32, PageFrame, PageContent)],
) -> BodyStyle {
    let mut characters: HashMap<(i64, bool), usize> = HashMap::new();
    for ((_, layout), (_, _, content)) in layouts.iter().zip(contents) {
        for item in &layout.items {
            match item {
                Item::Paragraph(lines) => {
                    for line in lines {
                        *characters
                            .entry((size_key(line.size), line.bold))
                            .or_default() += line.text.chars().count();
                    }
                }
                Item::Table { cells, .. } => {
                    for &run in cells.iter().flatten().flatten() {
                        let run = &content.texts[run];
                        *characters
                            .entry((size_key(run.font_size), run.bold))
                            .or_default() += run.text.trim().chars().count();
                    }
                }
                Item::Figure { .. } => {}
            }
        }
    }
    characters
        .into_iter()
        .max_by_key(|&(_, count)| count)
        .map(|((size, bold), _)| BodyStyle {
            size: size as f32 / 2.0,
            bold,
        })
        .unwrap_or(BodyStyle {
            size: 12.0,
            bold: false,
        })
}

/// whether a paragraph looks like a heading next to body text.
fn is_heading(lines: &[Line], body: &BodyStyle) -> bool {
    let text: String = lines
        .iter()
        .map(|l| l.text.as_str())
        .collect::<Vec<_>>()
        .join(" ");
    let size = lines.iter().map(|l| l.size).fold(0.0, f32::max);
    let bold = lines.iter().all(|l| l.bold);
    if lines.len() > 3 || text.chars().count() > 200 || crate::rules::list_label(&text).is_some() {
        return false;
    }
    let bigger = size >= body.size * 1.2;
    let bold_like_a_title = bold
        && !body.bold
        && size >= body.size * 0.95
        && lines.len() <= 2
        && !text.trim_end().ends_with(['.', ',', ';', ':']);
    bigger || bold_like_a_title
}

fn build_plan(
    doc: &Document,
    pages: &[u32],
    eligible: &dyn Fn(Mark) -> bool,
) -> Result<Plan, A11yError> {
    let ids = page_ids(doc);
    let mut contents = Vec::new();
    for &page in pages {
        let id = ids[page as usize];
        let frame = PageFrame::read(doc, id);
        let content =
            read_page(doc, id, &frame).map_err(|_| A11yError::UnreadableContent(page + 1))?;
        contents.push((page, frame, content));
    }
    let repeated = repeated_margin_texts(&contents, eligible);
    let layouts: Vec<(u32, PageLayout)> = contents
        .iter()
        .map(|(page, frame, content)| {
            (
                *page,
                analyse_page(content, frame.visible_size(), eligible, &repeated),
            )
        })
        .collect();
    let body = body_style(&layouts, &contents);
    let levels = heading_levels(&layouts, &body);

    let mut plan = Plan {
        nodes: Vec::new(),
        pages: Vec::new(),
        decisions: Vec::new(),
        counts: TagCounts::default(),
    };
    for ((page, _, content), (_, layout)) in contents.into_iter().zip(layouts) {
        let mut wraps = BTreeMap::new();
        plan_page(
            &mut plan, page, &content, &layout, &body, &levels, &mut wraps,
        );
        plan_leftovers(&mut plan, page, &content, &layout, eligible, &mut wraps);
        plan.pages.push(PagePlan {
            page,
            content,
            wraps,
        });
    }
    if !levels.is_empty() {
        plan.decide(
            0,
            None,
            None,
            Confidence::Low,
            format!(
                "Heading levels come from text size: {} different heading sizes were found. Check that each heading’s level matches its place in the document.",
                levels.len()
            ),
        );
    }
    Ok(plan)
}

/// margin texts that appear on more than one page: running headers and
/// footers.
fn repeated_margin_texts(
    contents: &[(u32, PageFrame, PageContent)],
    eligible: &dyn Fn(Mark) -> bool,
) -> HashSet<String> {
    let mut pages_with: HashMap<String, usize> = HashMap::new();
    for (_, frame, content) in contents {
        let (_, height) = frame.visible_size();
        let runs: Vec<usize> = (0..content.texts.len())
            .filter(|&r| {
                eligible(content.texts[r].mark) && !content.texts[r].text.trim().is_empty()
            })
            .collect();
        let keys: HashSet<String> = group_lines(content, &runs)
            .into_iter()
            .filter(|line| margin_band(&line.rect, height).is_some())
            .map(|line| margin_key(&line.text))
            .collect();
        for key in keys {
            *pages_with.entry(key).or_default() += 1;
        }
    }
    pages_with
        .into_iter()
        .filter(|(key, count)| *count >= 2 && !key.is_empty())
        .map(|(key, _)| key)
        .collect()
}

/// heading styles ranked into levels: biggest first, bold before plain at
/// the same size.
fn heading_levels(layouts: &[(u32, PageLayout)], body: &BodyStyle) -> HashMap<(i64, bool), u8> {
    let mut styles: Vec<(i64, bool)> = Vec::new();
    for (_, layout) in layouts {
        for item in &layout.items {
            if let Item::Paragraph(lines) = item {
                if is_heading(lines, body) {
                    let style = heading_style(lines);
                    if !styles.contains(&style) {
                        styles.push(style);
                    }
                }
            }
        }
    }
    styles.sort_by(|a, b| b.cmp(a));
    styles
        .into_iter()
        .enumerate()
        .map(|(rank, style)| (style, (rank + 1).min(6) as u8))
        .collect()
}

fn heading_style(lines: &[Line]) -> (i64, bool) {
    let size = lines.iter().map(|l| l.size).fold(0.0, f32::max);
    (size_key(size), lines.iter().all(|l| l.bold))
}

fn assign(wraps: &mut BTreeMap<(usize, usize), Wrap>, unit: (usize, usize), wrap: Wrap) {
    wraps.entry(unit).or_insert(wrap);
}

fn tag_runs(
    plan: &Plan,
    wraps: &mut BTreeMap<(usize, usize), Wrap>,
    content: &PageContent,
    runs: &[usize],
    leaf: usize,
) {
    let role = plan.nodes[leaf].role.clone();
    for &run in runs {
        assign(
            wraps,
            text_unit(content, run),
            Wrap::Tag {
                target: leaf,
                role: role.clone(),
            },
        );
    }
}

#[allow(clippy::too_many_arguments)]
fn plan_page(
    plan: &mut Plan,
    page: u32,
    content: &PageContent,
    layout: &PageLayout,
    body: &BodyStyle,
    levels: &HashMap<(i64, bool), u8>,
    wraps: &mut BTreeMap<(usize, usize), Wrap>,
) {
    let mut index = 0;
    while index < layout.items.len() {
        match &layout.items[index] {
            Item::Paragraph(lines) => {
                // a run of two or more list items becomes one list
                let list_end = list_run_end(&layout.items, index, body);
                if list_end > index + 1 {
                    plan_list(plan, page, content, &layout.items[index..list_end], wraps);
                    index = list_end;
                    continue;
                }
                let runs: Vec<usize> = lines.iter().flat_map(|l| l.runs.iter().copied()).collect();
                let role = if is_heading(lines, body) {
                    let level = levels.get(&heading_style(lines)).copied().unwrap_or(1);
                    plan.counts.headings += 1;
                    format!("H{level}")
                } else {
                    plan.counts.paragraphs += 1;
                    "P".to_owned()
                };
                let leaf = plan.node(&role, None, page);
                tag_runs(plan, wraps, content, &runs, leaf);
            }
            Item::Figure { image, rect } => {
                let leaf = plan.node("Figure", None, page);
                assign(
                    wraps,
                    image_unit(content, *image),
                    Wrap::Tag {
                        target: leaf,
                        role: "Figure".into(),
                    },
                );
                plan.counts.figures += 1;
                plan.decide(
                    page,
                    Some(*rect),
                    Some(leaf),
                    Confidence::Medium,
                    format!("Tagged a picture on page {} as a figure. It needs alt text, or mark it as decoration.", page + 1),
                );
            }
            Item::Table { grid, cells } => {
                let table = plan.node("Table", None, page);
                for (row_number, row) in cells.iter().enumerate() {
                    let row_node = plan.node("TR", Some(table), page);
                    for cell in row {
                        let role = if row_number == 0 { "TH" } else { "TD" };
                        let leaf = plan.node(role, Some(row_node), page);
                        if row_number == 0 {
                            plan.nodes[leaf].scope = Some("Column");
                        }
                        tag_runs(plan, wraps, content, cell, leaf);
                    }
                }
                plan.counts.tables += 1;
                plan.decide(
                    page,
                    Some(grid.rect()),
                    Some(table),
                    Confidence::Low,
                    format!(
                        "Tagged the ruled grid on page {} as a table of {} rows and {} columns, with the first row as column headers. Check the header cells.",
                        page + 1,
                        grid.row_count(),
                        grid.column_count()
                    ),
                );
            }
        }
        index += 1;
    }
}

/// where a run of consecutive list-item paragraphs starting at `start` ends.
fn list_run_end(items: &[Item], start: usize, body: &BodyStyle) -> usize {
    let mut end = start;
    while let Some(Item::Paragraph(lines)) = items.get(end) {
        let first = &lines[0].text;
        if crate::rules::list_label(first).is_none() || is_heading(lines, body) {
            break;
        }
        end += 1;
    }
    end
}

fn plan_list(
    plan: &mut Plan,
    page: u32,
    content: &PageContent,
    items: &[Item],
    wraps: &mut BTreeMap<(usize, usize), Wrap>,
) {
    let list = plan.node("L", None, page);
    plan.counts.lists += 1;
    let mut joined_labels = 0;
    for item in items {
        let Item::Paragraph(lines) = item else {
            continue;
        };
        let entry = plan.node("LI", Some(list), page);
        let runs: Vec<usize> = lines.iter().flat_map(|l| l.runs.iter().copied()).collect();
        let label_length = crate::rules::list_label(&lines[0].text).unwrap_or(0);
        let first = runs[0];
        let first_text = content.texts[first].text.trim();
        // the label can only be its own tag when it was drawn on its own
        if runs.len() > 1 && !first_text.is_empty() && first_text.chars().count() <= label_length {
            let label = plan.node("Lbl", Some(entry), page);
            tag_runs(plan, wraps, content, &[first], label);
            let body = plan.node("LBody", Some(entry), page);
            tag_runs(plan, wraps, content, &runs[1..], body);
        } else {
            let body = plan.node("LBody", Some(entry), page);
            tag_runs(plan, wraps, content, &runs, body);
            joined_labels += 1;
        }
    }
    if joined_labels > 0 {
        plan.decide(
            page,
            Some(Rect::union_all(items.iter().map(|i| i.rect()).collect::<Vec<_>>().iter()).unwrap_or_default()),
            Some(list),
            Confidence::Medium,
            format!(
                "Tagged a list of {} items on page {}. In {joined_labels} items the bullet or number is drawn together with the text, so it is part of the item’s body.",
                items.len(),
                page + 1
            ),
        );
    }
}

/// decoration, blank text and anything else on the page that the layout
/// didn't place.
fn plan_leftovers(
    plan: &mut Plan,
    page: u32,
    content: &PageContent,
    layout: &PageLayout,
    eligible: &dyn Fn(Mark) -> bool,
    wraps: &mut BTreeMap<(usize, usize), Wrap>,
) {
    let before = wraps.len();
    for &(run, kind) in &layout.margin_runs {
        assign(wraps, text_unit(content, run), Wrap::Artifact(kind));
    }
    for &image in layout
        .decorative_images
        .iter()
        .chain(&layout.background_images)
    {
        assign(
            wraps,
            image_unit(content, image),
            Wrap::Artifact(ArtifactKind::Layout),
        );
    }
    if let Some(&image) = layout.background_images.first() {
        plan.decide(
            page,
            Some(content.images[image].rect),
            None,
            Confidence::Low,
            format!("Treated the large picture behind the text on page {} as the picture of a scanned page and marked it as decoration; the text over it was tagged instead.", page + 1),
        );
    }
    for path in content.paths.iter().filter(|p| eligible(p.mark)) {
        let unit = if path.in_form {
            (path.op_start, path.op_start)
        } else {
            (path.op_start, path.op_end)
        };
        assign(wraps, unit, Wrap::Artifact(ArtifactKind::Layout));
    }
    // blank runs go with the text drawn just before them
    for &run in &layout.blank_runs {
        let unit = text_unit(content, run);
        let previous = wraps
            .range(..unit)
            .rev()
            .find(|(_, wrap)| matches!(wrap, Wrap::Tag { .. }))
            .map(|(_, wrap)| wrap.clone());
        assign(
            wraps,
            unit,
            previous.unwrap_or(Wrap::Artifact(ArtifactKind::Layout)),
        );
    }
    // anything eligible still unplaced is decoration rather than left untagged
    for run in (0..content.texts.len()).filter(|&r| eligible(content.texts[r].mark)) {
        assign(
            wraps,
            text_unit(content, run),
            Wrap::Artifact(ArtifactKind::Layout),
        );
    }
    for image in (0..content.images.len()).filter(|&i| eligible(content.images[i].mark)) {
        assign(
            wraps,
            image_unit(content, image),
            Wrap::Artifact(ArtifactKind::Layout),
        );
    }
    plan.counts.artifacts += wraps.len() - before;
}

// ---- writing ------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq)]
enum Mode {
    /// a new tree replacing any old one
    Create,
    /// new tags added to the existing tree
    Append,
}

/// takes out the old structure tree and the keys that pointed into it.
fn remove_old_tree(doc: &mut Document) -> Result<(), A11yError> {
    if let Some(tree) = StructTree::read(doc) {
        let old: Vec<ObjectId> = tree
            .elements
            .iter()
            .filter_map(|e| e.id)
            .chain(tree.root)
            .collect();
        for id in old {
            doc.objects.remove(&id);
        }
    }
    for page in page_ids(doc) {
        let annotations: Vec<ObjectId> = doc
            .get_dictionary(page)
            .ok()
            .and_then(|dict| crate::objects::get_array(doc, dict, b"Annots"))
            .unwrap_or_default()
            .iter()
            .filter_map(|a| a.as_reference().ok())
            .collect();
        for annotation in annotations {
            if let Ok(dict) = dict_mut(doc, annotation) {
                dict.remove(b"StructParent");
            }
        }
    }
    Ok(())
}

fn write_plan(doc: &mut Document, plan: Plan, mode: Mode) -> Result<AutoTagResult, A11yError> {
    let ids = page_ids(doc);
    // first the content: every page gets its marked content
    let mut owners: Vec<(ObjectId, Vec<(i64, usize)>)> = Vec::new();
    for page_plan in &plan.pages {
        let page_id = ids[page_plan.page as usize];
        let units: Vec<Unit> = page_plan
            .wraps
            .iter()
            .map(|(&(start, end), wrap)| Unit {
                start,
                end,
                wrap: wrap.clone(),
            })
            .collect();
        let mut next_mcid = match mode {
            Mode::Create => 0,
            Mode::Append => page_plan.content.max_mcid().map_or(0, |m| m + 1),
        };
        let (operations, page_owners) = wrap_units(
            &page_plan.content.operations,
            &units,
            &mut next_mcid,
            mode == Mode::Create,
        );
        replace_content(doc, page_id, operations)?;
        owners.push((page_id, page_owners));
    }
    // then the tree
    let mut writer = match mode {
        Mode::Create => TagWriter::create(doc)?,
        Mode::Append => TagWriter::open(doc)?,
    };
    let container = match mode {
        Mode::Create => {
            let root = writer.root;
            writer.new_element("Document", root, None)
        }
        Mode::Append => writer.top_container(),
    };
    let mut objects: Vec<ObjectId> = Vec::with_capacity(plan.nodes.len());
    for node in &plan.nodes {
        let parent = node.parent.map_or(container, |p| objects[p]);
        let element = writer.new_element(&node.role, parent, Some(ids[node.page as usize]));
        if let Some(scope) = node.scope {
            let mut attributes = lopdf::Dictionary::new();
            attributes.set("O", name("Table"));
            attributes.set("Scope", name(scope));
            if let Ok(dict) = dict_mut(writer.doc, element) {
                dict.set("A", Object::Dictionary(attributes));
            }
        }
        objects.push(element);
    }
    for (page_id, page_owners) in owners {
        for (mcid, node) in page_owners {
            writer.add_content(objects[node], page_id, mcid);
        }
    }
    writer.finish()?;

    let counts = plan.counts;
    let decisions = plan
        .decisions
        .into_iter()
        .map(|(mut decision, node)| {
            decision.element = node.map(|n| ObjectRef::from(objects[n]));
            decision
        })
        .collect();
    let mut changes = Vec::new();
    let made = [
        (counts.headings, "heading", "headings"),
        (counts.paragraphs, "paragraph", "paragraphs"),
        (counts.lists, "list", "lists"),
        (counts.figures, "figure", "figures"),
        (counts.tables, "table", "tables"),
    ]
    .iter()
    .filter(|(count, _, _)| *count > 0)
    .map(|(count, one, many)| crate::rules::plural(*count, one, many))
    .collect::<Vec<_>>();
    if !made.is_empty() {
        changes.push(format!("Tagged {}.", join_list(&made)));
    }
    if counts.artifacts > 0 {
        changes.push(format!(
            "Marked {} as decoration (page numbers, running headers and footers, lines and backgrounds).",
            crate::rules::plural(counts.artifacts, "piece of content", "pieces of content")
        ));
    }
    Ok(AutoTagResult {
        changes,
        decisions,
        counts,
    })
}

/// "a, b and c".
fn join_list(items: &[String]) -> String {
    match items {
        [] => String::new(),
        [one] => one.clone(),
        [rest @ .., last] => format!("{} and {last}", rest.join(", ")),
    }
}

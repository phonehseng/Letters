//! true redaction: the content under each box is taken out of the file,
//! then the box is drawn where it was.
//!
//! every operation works in two steps. first the whole request is planned
//! against the unchanged document: each page's content is read, walked
//! and rewritten in memory, and any error stops everything here, with
//! nothing changed. only then is the plan applied. a preview is the same
//! planning step with the result described instead of applied.

mod annotations;
mod images;
mod marks;
pub(crate) mod paths;
mod structure;
mod text;
pub(crate) mod walker;
mod xobjects;

use std::collections::{BTreeMap, HashMap, HashSet};

use lopdf::{Document, Object, ObjectId};
use serde::{Deserialize, Serialize};

use crate::cleanup::{remove_document_extras, remove_page_extras};
use crate::content::{self, Op};
use crate::error::RedactError;
use crate::geometry::Matrix;
use crate::objects::{decoded_content, flate_stream, reference_counts, resolve, NewObjects};
use crate::page::{page_ids, page_resources, PageFrame, PageRect};
use crate::text_layout::{lay_out, PlacedGlyph};
use crate::warnings::{find_leftovers, Leftover};
use annotations::{find_hits, remove_hits, AnnotationHit};
use marks::{areas_without_marks, label_font, mark_ops, split_marks, unwrap_body, MarkStyle};
use structure::{clean_structure, Touched};
use walker::{Context, FontCache, GlyphRecord, WalkMode, WalkSettings, Walker};
use xobjects::merge_resources;

/// the boxes to redact on one page.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RedactionArea {
    /// zero based page number
    pub page: u32,
    /// boxes in points from the visible top left corner of the page
    pub rects: Vec<PageRect>,
}

/// a colour as red, green and blue from 0 to 255.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Rgb {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

impl Rgb {
    pub const BLACK: Rgb = Rgb { r: 0, g: 0, b: 0 };

    fn fractions(self) -> [f64; 3] {
        [self.r, self.g, self.b].map(|value| f64::from(value) / 255.0)
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct RedactOptions {
    /// the colour of the boxes, black unless chosen
    pub fill: Rgb,
    /// a short label drawn in each box, such as "Redacted"
    pub overlay_text: Option<String>,
    /// keep vector paths that cross a box and clip them, instead of
    /// removing them whole. off by default: a clipped path's coordinates
    /// stay in the file (see `paths.rs`)
    pub clip_crossing_paths: bool,
}

impl Default for RedactOptions {
    fn default() -> Self {
        Self {
            fill: Rgb::BLACK,
            overlay_text: None,
            clip_crossing_paths: false,
        }
    }
}

/// what was removed from one page.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PageReport {
    pub page: u32,
    pub glyphs_removed: usize,
    pub images_removed: usize,
    /// images kept with the covered pixels replaced
    pub images_rewritten: usize,
    pub paths_removed: usize,
    /// filled rectangles cut around the boxes
    pub paths_cut: usize,
    /// paths kept and clipped (only with `clip_crossing_paths`)
    pub paths_clipped: usize,
    /// gradients clipped so they paint nothing in the boxes
    pub shadings_clipped: usize,
    pub annotations_removed: usize,
    pub form_values_removed: usize,
    /// things removed whole because they could not be cut exactly, as
    /// sentences the app can show
    pub removed_wholesale: Vec<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RedactionReport {
    pub pages: Vec<PageReport>,
    /// places that may still show what was removed, which redaction does
    /// not change (other pages, bookmarks, the document title...)
    pub warnings: Vec<String>,
}

/// what one box would remove.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AreaPreview {
    pub page: u32,
    pub rect: PageRect,
    /// the removed text, one string per line
    pub texts: Vec<String>,
    pub images: usize,
    pub paths: usize,
    /// annotations that would go, described for people
    pub annotations: Vec<String>,
    /// form field values that would go
    pub form_values: Vec<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RedactionPreview {
    pub areas: Vec<AreaPreview>,
    pub report: RedactionReport,
}

/// removes everything under the boxes and draws the boxes.
///
/// the document must be saved with [`crate::save_clean`] afterwards. an
/// incremental save would keep the old content in the file, after the
/// new, where anyone can read it.
pub fn redact(
    doc: &mut Document,
    areas: &[RedactionArea],
    options: &RedactOptions,
) -> Result<RedactionReport, RedactError> {
    ensure_unlocked(doc)?;
    let flat = flatten_areas(doc, areas)?;
    let plan = plan(doc, &flat, options, WalkMode::Redact)?;
    Ok(apply(doc, plan))
}

/// works out what [`redact`] with the default options would remove,
/// without changing anything. see [`preview_with_options`].
pub fn preview(doc: &Document, areas: &[RedactionArea]) -> Result<RedactionPreview, RedactError> {
    preview_with_options(doc, areas, &RedactOptions::default())
}

/// works out what [`redact`] would remove and report, without changing
/// the document.
///
/// the redaction is really carried out, on a copy of the document. that
/// costs the memory of a second copy and the time of rewriting images,
/// but it is the only way the warnings (which look for the removed text
/// in the finished file) and the notes about things removed whole (which
/// depend on actually decoding each image) come out exactly as `redact`
/// gives them. a preview that disagreed with the result would be worse
/// than none.
pub fn preview_with_options(
    doc: &Document,
    areas: &[RedactionArea],
    options: &RedactOptions,
) -> Result<RedactionPreview, RedactError> {
    ensure_unlocked(doc)?;
    let flat = flatten_areas(doc, areas)?;
    let mut copy = doc.clone();
    let plan = plan(&copy, &flat, options, WalkMode::Redact)?;
    let previews = describe_areas(&flat, &plan);
    let report = apply(&mut copy, plan);
    Ok(RedactionPreview {
        areas: previews,
        report,
    })
}

pub(crate) fn ensure_unlocked(doc: &Document) -> Result<(), RedactError> {
    if doc.is_encrypted() {
        return Err(RedactError::Locked);
    }
    Ok(())
}

/// one box, with its page and number in the request.
struct FlatArea {
    page: u32,
    rect: PageRect,
}

fn flatten_areas(doc: &Document, areas: &[RedactionArea]) -> Result<Vec<FlatArea>, RedactError> {
    let page_count = doc.get_pages().len() as u32;
    let mut flat = Vec::new();
    for area in areas {
        if area.page >= page_count {
            return Err(RedactError::UnknownPage(RedactError::page_number(
                area.page,
            )));
        }
        for rect in &area.rects {
            let usable = rect.width > 0.0
                && rect.height > 0.0
                && [rect.x, rect.y, rect.width, rect.height]
                    .iter()
                    .all(|v| v.is_finite());
            if !usable {
                return Err(RedactError::EmptyArea(RedactError::page_number(area.page)));
            }
            flat.push(FlatArea {
                page: area.page,
                rect: *rect,
            });
        }
    }
    Ok(flat)
}

/// the planned change to one page.
struct PagePlan {
    page: u32,
    page_id: ObjectId,
    /// the page's new content, if it changes
    content: Option<Vec<u8>>,
    additions: BTreeMap<Vec<u8>, Vec<(Vec<u8>, Object)>>,
    /// xobject names the new content no longer draws
    unused_xobjects: Vec<Vec<u8>>,
    findings: walker::Findings,
    annotation_hits: Vec<AnnotationHit>,
}

struct Plan {
    pages: Vec<PagePlan>,
    new_objects: NewObjects,
}

fn plan(
    doc: &Document,
    areas: &[FlatArea],
    options: &RedactOptions,
    mode: WalkMode,
) -> Result<Plan, RedactError> {
    let pages = page_ids(doc);
    let counts = reference_counts(doc);
    let mut by_page: BTreeMap<u32, Vec<usize>> = BTreeMap::new();
    for (index, area) in areas.iter().enumerate() {
        by_page.entry(area.page).or_default().push(index);
    }
    let mut new_objects = NewObjects::after(doc);
    let mut fonts = FontCache::default();
    let mut plans = Vec::new();
    for (page, indices) in by_page {
        let page_id = pages[page as usize];
        let frame = PageFrame::of_page(doc, page_id);
        let settings = WalkSettings {
            mode,
            areas: indices
                .iter()
                .map(|&i| frame.to_user(&areas[i].rect))
                .collect(),
            area_ids: indices.clone(),
            fill: options.fill.fractions(),
            clip_paths: options.clip_crossing_paths,
        };
        let style = MarkStyle {
            fill: settings.fill,
            label: options
                .overlay_text
                .clone()
                .filter(|label| !label.trim().is_empty()),
        };
        let (page_plan, objects, cache) = plan_page(
            doc,
            page,
            page_id,
            &frame,
            &settings,
            &style,
            &counts,
            new_objects,
            fonts,
        )?;
        new_objects = objects;
        fonts = cache;
        plans.push(page_plan);
    }
    Ok(Plan {
        pages: plans,
        new_objects,
    })
}

#[allow(clippy::too_many_arguments)]
fn plan_page(
    doc: &Document,
    page: u32,
    page_id: ObjectId,
    frame: &PageFrame,
    settings: &WalkSettings,
    style: &MarkStyle,
    counts: &HashMap<ObjectId, usize>,
    new_objects: NewObjects,
    fonts: FontCache,
) -> Result<(PagePlan, NewObjects, FontCache), RedactError> {
    let unreadable =
        |reason: String| RedactError::UnreadablePage(RedactError::page_number(page), reason);
    let bytes = page_content(doc, page_id).map_err(unreadable)?;
    let ops = content::parse(&bytes)
        .map_err(|reason| unreadable(format!("its drawing instructions are damaged: {reason}")))?;
    let (body, old_marks, marked) = split_marks(ops);
    let body = unwrap_body(body);
    let resources = page_resources(doc, page_id);
    let mut ctx = Context {
        doc,
        settings,
        new_objects,
        fonts,
        findings: Default::default(),
        reference_counts: counts,
    };
    let output = Walker::new(&mut ctx, resources.clone(), Matrix::IDENTITY, 0)
        .run(body)
        .map_err(unreadable)?;
    let mut additions = output.additions;
    let fresh = areas_without_marks(&settings.areas, &marked);
    // the label font is only added when a new box needs it, so redacting
    // the same boxes again leaves the page exactly as it was
    let font_name = style
        .label
        .as_ref()
        .filter(|_| !fresh.is_empty())
        .map(|_| add_label_font(doc, &resources, &mut additions));
    let new_marks = mark_ops(&fresh, style, frame, font_name.as_deref());
    let content = (output.changed || !new_marks.is_empty()).then(|| {
        let mut ops = vec![Op::new("q", vec![])];
        ops.extend(output.ops);
        ops.extend((0..output.open_saves + 1).map(|_| Op::new("Q", vec![])));
        ops.extend(old_marks);
        ops.extend(new_marks);
        content::serialize(&ops)
    });
    let annotation_hits = find_hits(doc, page_id, &settings.areas, &settings.area_ids);
    let plan = PagePlan {
        page,
        page_id,
        content,
        additions,
        unused_xobjects: output.unused_xobjects,
        findings: ctx.findings,
        annotation_hits,
    };
    Ok((plan, ctx.new_objects, ctx.fonts))
}

fn add_label_font(
    doc: &Document,
    resources: &lopdf::Dictionary,
    additions: &mut BTreeMap<Vec<u8>, Vec<(Vec<u8>, Object)>>,
) -> Vec<u8> {
    let existing: Vec<Vec<u8>> = match resources.get(b"Font").ok().map(|o| resolve(doc, o)) {
        Some(Object::Dictionary(fonts)) => fonts.iter().map(|(key, _)| key.clone()).collect(),
        _ => Vec::new(),
    };
    let added = additions.entry(b"Font".to_vec()).or_default();
    let mut number = 1;
    let name = loop {
        let candidate = format!("LRLabel{number}").into_bytes();
        if !existing.contains(&candidate) && !added.iter().any(|(key, _)| *key == candidate) {
            break candidate;
        }
        number += 1;
    };
    added.push((name.clone(), Object::Dictionary(label_font())));
    name
}

/// a page's content streams, joined as one (pdf allows a page's content
/// to be split anywhere between operations, even across streams).
pub(crate) fn page_content(doc: &Document, page_id: ObjectId) -> Result<Vec<u8>, String> {
    let page = doc
        .get_dictionary(page_id)
        .map_err(|error| error.to_string())?;
    let streams: Vec<Object> = match page.get(b"Contents").ok().map(|c| resolve(doc, c)) {
        Some(Object::Array(items)) => items.clone(),
        Some(Object::Stream(_)) => vec![page.get(b"Contents").cloned().unwrap_or(Object::Null)],
        _ => Vec::new(),
    };
    let mut bytes = Vec::new();
    for entry in streams {
        match resolve(doc, &entry) {
            Object::Stream(stream) => {
                let decoded = decoded_content(stream)
                    .map_err(|error| format!("its content can't be decompressed ({error})"))?;
                bytes.extend_from_slice(&decoded);
                bytes.push(b'\n');
            }
            Object::Null => {}
            _ => return Err("its content is not a stream".into()),
        }
    }
    Ok(bytes)
}

fn apply(doc: &mut Document, plan: Plan) -> RedactionReport {
    let counts = reference_counts(doc);
    plan.new_objects.commit(doc);
    let mut report = RedactionReport::default();
    let mut touched_content = HashSet::new();
    let mut removed_objects = HashSet::new();
    let mut removed_text = Vec::new();
    let mut shared_images = Vec::new();
    for page in plan.pages {
        let removal = remove_hits(doc, page.page_id, &page.annotation_hits);
        if let Some(content) = page.content {
            write_page_content(doc, page.page_id, &content, &counts);
        }
        if !page.additions.is_empty() || !page.unused_xobjects.is_empty() {
            let merged = merge_resources(
                doc,
                &page_resources(doc, page.page_id),
                &page.additions,
                &page.unused_xobjects,
            );
            if let Ok(dict) = doc.get_dictionary_mut(page.page_id) {
                dict.set("Resources", Object::Dictionary(merged));
            }
        }
        remove_page_extras(doc, page.page_id);
        touched_content.extend(
            page.findings
                .touched_mcids
                .iter()
                .map(|mcid| (page.page_id, *mcid)),
        );
        removed_objects.extend(removal.removed_ids.iter().copied());
        removed_text.extend(text_lines(&page.findings.glyphs));
        shared_images.extend(page.findings.shared_images.iter().copied());
        report.warnings.extend(removal.warnings.iter().cloned());
        report
            .pages
            .push(page_report(page.page, &page.findings, &removal));
    }
    clean_structure(
        doc,
        &Touched {
            content: &touched_content,
            objects: &removed_objects,
        },
    );
    report.warnings.extend(remove_document_extras(doc));
    let leftovers = Leftover {
        removed_text: &removed_text,
        shared_images: &shared_images,
    };
    report.warnings.extend(find_leftovers(doc, &leftovers));
    dedupe(&mut report.warnings);
    report
}

/// replaces a page's content. a stream that only this page uses is
/// rewritten where it is; a shared one is left for the other pages and
/// the page gets a new stream.
fn write_page_content(
    doc: &mut Document,
    page_id: ObjectId,
    content: &[u8],
    counts: &HashMap<ObjectId, usize>,
) {
    let stream = flate_stream(lopdf::Dictionary::new(), content);
    let current = doc
        .get_dictionary(page_id)
        .ok()
        .and_then(|page| page.get(b"Contents").ok())
        .and_then(|contents| contents.as_reference().ok());
    let reusable = current.filter(|id| {
        counts.get(id).copied().unwrap_or(0) <= 1
            && matches!(doc.objects.get(id), Some(Object::Stream(_)))
    });
    let id = match reusable {
        Some(id) => {
            doc.objects.insert(id, Object::Stream(stream));
            id
        }
        None => doc.add_object(Object::Stream(stream)),
    };
    if let Ok(page) = doc.get_dictionary_mut(page_id) {
        page.set("Contents", Object::Reference(id));
    }
}

fn page_report(
    page: u32,
    findings: &walker::Findings,
    removal: &annotations::AnnotationRemoval,
) -> PageReport {
    let counts = &findings.counts;
    PageReport {
        page,
        glyphs_removed: counts.glyphs_removed,
        images_removed: counts.images_removed,
        images_rewritten: counts.images_rewritten,
        paths_removed: counts.paths_removed,
        paths_cut: counts.paths_cut,
        paths_clipped: counts.paths_clipped,
        shadings_clipped: counts.shadings_clipped,
        annotations_removed: removal.annotations,
        form_values_removed: removal.field_values,
        removed_wholesale: findings.wholesale.clone(),
    }
}

/// what each area would remove, from a plan made before it is applied.
fn describe_areas(areas: &[FlatArea], plan: &Plan) -> Vec<AreaPreview> {
    let mut previews: Vec<AreaPreview> = areas
        .iter()
        .map(|area| AreaPreview {
            page: area.page,
            rect: area.rect,
            ..Default::default()
        })
        .collect();
    for page in &plan.pages {
        for (index, preview) in previews.iter_mut().enumerate() {
            let glyphs: Vec<GlyphRecord> = page
                .findings
                .glyphs
                .iter()
                .filter(|g| g.area == Some(index))
                .cloned()
                .collect();
            if !glyphs.is_empty() {
                preview.texts = text_lines(&glyphs);
            }
            if let Some(tally) = page.findings.per_area.get(&index) {
                preview.images = tally.images;
                preview.paths = tally.paths;
            }
            for hit in page.annotation_hits.iter().filter(|hit| hit.area == index) {
                preview.annotations.push(hit.description.clone());
                preview.form_values.extend(hit.field_value.clone());
            }
        }
    }
    previews
}
/// removed glyphs joined into lines of text.
fn text_lines(glyphs: &[GlyphRecord]) -> Vec<String> {
    let placed: Vec<PlacedGlyph> = glyphs
        .iter()
        .map(|g| PlacedGlyph {
            text: &g.text,
            quad: &g.quad,
        })
        .collect();
    lay_out(&placed)
        .text
        .lines()
        .map(|line| line.trim().to_owned())
        .filter(|line| !line.is_empty())
        .collect()
}

fn dedupe(lines: &mut Vec<String>) {
    let mut seen = HashSet::new();
    lines.retain(|line| seen.insert(line.clone()));
}

/// every glyph a page draws, with its box, for searching.
pub(crate) fn collect_glyphs(
    doc: &Document,
    page_id: ObjectId,
) -> Result<Vec<GlyphRecord>, String> {
    let bytes = page_content(doc, page_id)?;
    let ops = content::parse(&bytes)?;
    let settings = WalkSettings {
        mode: WalkMode::Collect,
        areas: Vec::new(),
        area_ids: Vec::new(),
        fill: [0.0; 3],
        clip_paths: false,
    };
    let counts = HashMap::new();
    let mut ctx = Context {
        doc,
        settings: &settings,
        new_objects: NewObjects::after(doc),
        fonts: FontCache::default(),
        findings: Default::default(),
        reference_counts: &counts,
    };
    Walker::new(&mut ctx, page_resources(doc, page_id), Matrix::IDENTITY, 0).run(ops)?;
    Ok(ctx.findings.glyphs)
}

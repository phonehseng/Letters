//! writing edited content streams back, touching only what changed.
//!
//! an edit is a plan of changes to single operators (replace it, or add
//! something before or after it) plus resources to add (a font, an
//! image). applying the plan rebuilds each changed stream from the
//! original bytes, so every operator the plan doesn't mention stays
//! exactly as it was.
//!
//! a form XObject (a reusable drawing) may be drawn in several places.
//! changing it in place would change every place, so a changed form is
//! always copied first and only the one place being edited is pointed at
//! the copy. a page content stream shared between pages is copied too.

use std::collections::{BTreeMap, HashSet};

use lopdf::{Dictionary, Document, Object, ObjectId, Stream};

use crate::draft::Draft;
use crate::error::EditError;
use crate::geometry::{Matrix, Point};
use crate::interpret::{Interpretation, Origin, Resources};
use crate::page_space::inherited;
use crate::pdf_values::resolve;
use crate::writer;

/// what happens to one operator.
#[derive(Clone, Debug, Default)]
pub(crate) struct OpEdit {
    pub before: Vec<u8>,
    /// new bytes for the operator itself; none keeps it
    pub replace: Option<Vec<u8>>,
    pub after: Vec<u8>,
}

/// a resource an edit needs: which stream uses it (none for content
/// added to the page), its category ("Font", "XObject") and the object.
#[derive(Clone, Debug)]
struct ResourceAdd {
    source: Option<usize>,
    category: &'static str,
    name: Vec<u8>,
    object: ObjectId,
}

/// every change an edit makes to a page's content.
#[derive(Default)]
pub(crate) struct Plan {
    ops: BTreeMap<(usize, usize), OpEdit>,
    resources: Vec<ResourceAdd>,
    /// whole new content drawn on top of, or underneath, the page
    append: Vec<u8>,
    prepend: Vec<u8>,
    /// page-space points new drawing reaches inside a form, so the form's
    /// /BBox (which clips it) can be made big enough
    reach: Vec<(usize, Point)>,
}

impl Plan {
    pub fn op(&mut self, source: usize, op: usize) -> &mut OpEdit {
        self.ops.entry((source, op)).or_default()
    }

    pub fn append(&mut self, content: &[u8]) {
        self.append.extend_from_slice(content);
        self.append.push(b'\n');
    }

    pub fn prepend(&mut self, content: &[u8]) {
        self.prepend.extend_from_slice(content);
        self.prepend.push(b'\n');
    }

    /// notes that new drawing in a stream reaches these page-space points.
    pub fn reach(&mut self, source: usize, points: &[Point]) {
        self.reach
            .extend(points.iter().map(|point| (source, *point)));
    }

    /// gives an object a resource name in a stream's resources (or the
    /// page's, for `None`), reusing the name it already has there.
    pub fn resource_name(
        &mut self,
        doc: &Document,
        run: &Interpretation,
        source: Option<usize>,
        category: &'static str,
        object: ObjectId,
    ) -> Vec<u8> {
        let existing = resources_of(doc, run, source).and_then(|resources| {
            let category = resolve(doc, resources.get(category.as_bytes()).ok()?)
                .as_dict()
                .ok()?;
            Some(category.clone())
        });
        if let Some(category) = &existing {
            if let Some((name, _)) = category
                .iter()
                .find(|(_, value)| value.as_reference().ok() == Some(object))
            {
                return name.clone();
            }
        }
        if let Some(added) = self
            .resources
            .iter()
            .find(|add| add.source == source && add.object == object && add.category == category)
        {
            return added.name.clone();
        }
        let taken = |name: &[u8]| {
            existing.as_ref().is_some_and(|category| category.has(name))
                || self
                    .resources
                    .iter()
                    .any(|add| add.source == source && add.category == category && add.name == name)
        };
        let prefix = if category == "Font" { "LtF" } else { "LtX" };
        let name = (1..)
            .map(|number| format!("{prefix}{number}").into_bytes())
            .find(|name| !taken(name))
            .expect("names never run out");
        self.resources.push(ResourceAdd {
            source,
            category,
            name: name.clone(),
            object,
        });
        name
    }
}

/// the resource dictionary a stream (or the page) uses.
fn resources_of<'a>(
    doc: &'a Document,
    run: &Interpretation,
    source: Option<usize>,
) -> Option<&'a Dictionary> {
    let page = doc.get_dictionary(run.page_id).ok()?;
    let kind = source.map_or(Resources::Page, |source| {
        run.sources[source].resources.clone()
    });
    match kind {
        Resources::Page => inherited(doc, page, b"Resources")
            .and_then(|resources| resolve(doc, resources).as_dict().ok()),
        Resources::Object(id) => doc.get_dictionary(id).ok(),
        Resources::InForm(form) => {
            let stream = doc.get_object(form).ok()?.as_stream().ok()?;
            resolve(doc, stream.dict.get(b"Resources").ok()?)
                .as_dict()
                .ok()
        }
    }
}

/// how many times anything in the file refers to an object. used to tell
/// whether a content stream belongs to one page or is shared.
fn reference_count(doc: &Document, target: ObjectId) -> usize {
    fn count(object: &Object, target: ObjectId) -> usize {
        match object {
            Object::Reference(id) => usize::from(*id == target),
            Object::Array(items) => items.iter().map(|item| count(item, target)).sum(),
            Object::Dictionary(dict) => dict.iter().map(|(_, value)| count(value, target)).sum(),
            Object::Stream(stream) => stream
                .dict
                .iter()
                .map(|(_, value)| count(value, target))
                .sum(),
            _ => 0,
        }
    }
    doc.objects
        .values()
        .map(|object| count(object, target))
        .sum()
}

/// the new bytes of a stream with the plan's operator edits applied.
fn rebuilt_content(run: &Interpretation, source: usize, plan: &Plan) -> Vec<u8> {
    let original = &run.sources[source];
    let mut content = Vec::with_capacity(original.content.len() + 256);
    let mut position = 0;
    for ((_, op), edit) in plan.ops.range((source, 0)..(source + 1, 0)) {
        let span = original.operations[*op].span.clone();
        content.extend_from_slice(&original.content[position..span.start]);
        if !edit.before.is_empty() {
            content.extend_from_slice(&edit.before);
            content.push(b'\n');
        }
        match &edit.replace {
            Some(replacement) => content.extend_from_slice(replacement),
            None => content.extend_from_slice(&original.content[span.clone()]),
        }
        if !edit.after.is_empty() {
            content.push(b'\n');
            content.extend_from_slice(&edit.after);
        }
        position = span.end;
    }
    content.extend_from_slice(&original.content[position..]);
    content
}

fn new_stream(mut dict: Dictionary, content: Vec<u8>) -> Stream {
    // the content is plain now, so earlier filters no longer apply
    dict.remove(b"Filter");
    dict.remove(b"DecodeParms");
    let mut stream = Stream::new(dict, content);
    let _ = stream.compress();
    stream
}

/// applies a plan to the draft. sources are handled deepest first, since
/// copying a form changes the stream that draws it.
pub(crate) fn apply(
    draft: &mut Draft,
    run: &Interpretation,
    mut plan: Plan,
) -> Result<(), EditError> {
    let edited: HashSet<usize> = plan.ops.keys().map(|(source, _)| *source).collect();
    let with_resources: HashSet<usize> =
        plan.resources.iter().filter_map(|add| add.source).collect();
    let mut pending: Vec<usize> = edited.union(&with_resources).copied().collect();
    pending.sort_unstable();
    while let Some(source) = pending.pop() {
        match run.sources[source].origin.clone() {
            Origin::Page { index } => write_page_stream(draft, run, &plan, source, index)?,
            Origin::Form { parent, op, .. } => {
                let copy = copy_form(draft, run, &plan, source)?;
                let name = plan.resource_name(draft.doc, run, Some(parent), "XObject", copy);
                plan.op(parent, op).replace = Some(writer::operation("Do", &[Object::Name(name)]));
                if !pending.contains(&parent) {
                    pending.push(parent);
                    pending.sort_unstable();
                }
            }
        }
    }
    let page_resources: Vec<ResourceAdd> = plan
        .resources
        .iter()
        .filter(|add| {
            add.source
                .is_none_or(|source| matches!(run.sources[source].resources, Resources::Page))
        })
        .cloned()
        .collect();
    add_resources(draft, run, None, &page_resources)?;
    let object_resources: Vec<ResourceAdd> = plan
        .resources
        .iter()
        .filter(|add| {
            add.source
                .is_some_and(|source| matches!(run.sources[source].resources, Resources::Object(_)))
        })
        .cloned()
        .collect();
    for add in &object_resources {
        add_resources(draft, run, add.source, std::slice::from_ref(add))?;
    }
    if !plan.append.is_empty() || !plan.prepend.is_empty() {
        add_page_content(draft, run, &plan.prepend, &plan.append)?;
    }
    Ok(())
}

fn write_page_stream(
    draft: &mut Draft,
    run: &Interpretation,
    plan: &Plan,
    source: usize,
    index: usize,
) -> Result<(), EditError> {
    let object = run.sources[source].object;
    let content = rebuilt_content(run, source, plan);
    let dict = draft
        .object(object)
        .and_then(|stream| stream.as_stream().ok())
        .map(|stream| stream.dict.clone())
        .ok_or_else(|| EditError::Damaged("a content stream is missing".into()))?;
    let stream = Object::Stream(new_stream(dict, content));
    if reference_count(draft.doc, object) <= 1 {
        draft.set(object, stream);
        return Ok(());
    }
    // another page uses this stream too: give this page its own copy
    let copy = draft.add(stream);
    let mut contents = crate::interpret::page_content_streams(draft.doc, page_dict(draft, run)?);
    contents[index] = copy;
    let mut page = page_dict(draft, run)?.clone();
    page.set(
        "Contents",
        Object::Array(contents.into_iter().map(Object::Reference).collect()),
    );
    draft.set(run.page_id, Object::Dictionary(page));
    Ok(())
}

fn page_dict<'a>(draft: &'a Draft, run: &Interpretation) -> Result<&'a Dictionary, EditError> {
    draft
        .dictionary(run.page_id)
        .ok_or_else(|| EditError::Damaged("the page is missing".into()))
}

/// copies a form with its edits and new resources; returns the copy.
fn copy_form(
    draft: &mut Draft,
    run: &Interpretation,
    plan: &Plan,
    source: usize,
) -> Result<ObjectId, EditError> {
    let original = draft
        .object(run.sources[source].object)
        .and_then(|form| form.as_stream().ok())
        .ok_or_else(|| EditError::Damaged("a drawing is missing".into()))?;
    let mut dict = original.dict.clone();
    let additions: Vec<&ResourceAdd> = plan
        .resources
        .iter()
        .filter(|add| add.source == Some(source))
        .collect();
    if !additions.is_empty() && !matches!(run.sources[source].resources, Resources::Object(_)) {
        // the copy gets its own resources: what it used plus the additions
        let mut resources = resources_of(draft.doc, run, Some(source))
            .cloned()
            .unwrap_or_default();
        for add in additions {
            add_to_category(draft.doc, &mut resources, add);
        }
        dict.set("Resources", Object::Dictionary(resources));
    }
    if let Some(bbox) = grown_bbox(draft.doc, run, plan, source, &dict) {
        dict.set("BBox", crate::pdf_values::number_array(&bbox));
    }
    let content = rebuilt_content(run, source, plan);
    Ok(draft.add(Object::Stream(new_stream(dict, content))))
}

/// a form's /BBox grown to hold the new drawing, which it would otherwise
/// cut off. none when nothing new reaches outside it.
fn grown_bbox(
    doc: &Document,
    run: &Interpretation,
    plan: &Plan,
    source: usize,
    dict: &Dictionary,
) -> Option<[f32; 4]> {
    let to_form = run.sources[source]
        .base_ctm
        .invert()
        .unwrap_or(Matrix::IDENTITY);
    let points: Vec<Point> = plan
        .reach
        .iter()
        .filter(|(of, _)| *of == source)
        .map(|(_, point)| to_form.apply(*point))
        .collect();
    if points.is_empty() {
        return None;
    }
    let mut bbox = dict
        .get(b"BBox")
        .ok()
        .and_then(|bbox| crate::pdf_values::rect_from_object(doc, bbox))
        .unwrap_or([0.0; 4]);
    for point in points {
        bbox = [
            bbox[0].min(point.x),
            bbox[1].min(point.y),
            bbox[2].max(point.x),
            bbox[3].max(point.y),
        ];
    }
    Some(bbox)
}

fn add_to_category(doc: &Document, resources: &mut Dictionary, add: &ResourceAdd) {
    let mut category = resources
        .get(add.category.as_bytes())
        .ok()
        .and_then(|category| resolve(doc, category).as_dict().ok())
        .cloned()
        .unwrap_or_default();
    category.set(add.name.clone(), Object::Reference(add.object));
    resources.set(add.category, Object::Dictionary(category));
}

/// writes resources into the page's dictionary (or a shared resources
/// object). inherited resources are copied onto the page first, so other
/// pages don't change; a shared object only ever gains new names.
fn add_resources(
    draft: &mut Draft,
    run: &Interpretation,
    source: Option<usize>,
    additions: &[ResourceAdd],
) -> Result<(), EditError> {
    if additions.is_empty() {
        return Ok(());
    }
    let target = match source.map(|source| run.sources[source].resources.clone()) {
        Some(Resources::Object(id)) => Some(id),
        _ => match page_dict(draft, run)?.get(b"Resources") {
            Ok(Object::Reference(id)) => Some(*id),
            _ => None,
        },
    };
    let mut resources = match target {
        Some(id) => draft.dictionary(id).cloned().unwrap_or_default(),
        None => resources_of(draft.doc, run, None)
            .cloned()
            .unwrap_or_default(),
    };
    for add in additions {
        add_to_category(draft.doc, &mut resources, add);
    }
    match target {
        Some(id) => draft.set(id, Object::Dictionary(resources)),
        None => {
            let mut page = page_dict(draft, run)?.clone();
            page.set("Resources", Object::Dictionary(resources));
            draft.set(run.page_id, Object::Dictionary(page));
        }
    }
    Ok(())
}

/// adds whole new content under or over the page. the page's own content
/// is put between q and Q, so whatever state it leaves behind can't move
/// the new content.
fn add_page_content(
    draft: &mut Draft,
    run: &Interpretation,
    prepend: &[u8],
    append: &[u8],
) -> Result<(), EditError> {
    let mut contents: Vec<Object> =
        crate::interpret::page_content_streams(draft.doc, page_dict(draft, run)?)
            .into_iter()
            .map(Object::Reference)
            .collect();
    let mut opening = b"q\n".to_vec();
    if !prepend.is_empty() {
        opening = [b"q\n".as_slice(), prepend, b"Q\nq\n"].concat();
    }
    let opening = draft.add(Object::Stream(new_stream(Dictionary::new(), opening)));
    let closing = draft.add(Object::Stream(new_stream(
        Dictionary::new(),
        [b"Q\n".as_slice(), append].concat(),
    )));
    contents.insert(0, Object::Reference(opening));
    contents.push(Object::Reference(closing));
    let mut page = page_dict(draft, run)?.clone();
    page.set("Contents", Object::Array(contents));
    draft.set(run.page_id, Object::Dictionary(page));
    Ok(())
}

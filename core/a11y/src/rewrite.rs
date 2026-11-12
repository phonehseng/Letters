//! changes a page's drawing instructions: wrapping content in marked
//! content sequences, turning tagged content into artifacts, and changing
//! the colour of chosen text.
//!
//! marked content (`BDC` … `EMC`) has to nest properly with text objects
//! (`BT` … `ET`), saved graphics states (`q` … `Q`) and other marked
//! content. so a sequence this module opens is always closed again before
//! any of those boundaries, and a new one (with a new mcid) is opened on
//! the other side when the same tag continues. one tag owning several
//! mcids is normal and every reader handles it.

use std::collections::HashSet;

use lopdf::content::{Content, Operation};
use lopdf::{Dictionary, Document, Object, ObjectId, Stream};

use crate::error::A11yError;
use crate::objects::{dict_mut, name, page_ids};

/// what to do with one unit of content.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Wrap {
    /// tag it: `/role <</MCID n>> BDC`, owned by structure leaf `target`
    Tag { target: usize, role: String },
    /// mark it as decoration
    Artifact(ArtifactKind),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ArtifactKind {
    /// lines, backgrounds and other layout decoration
    Layout,
    /// a running header
    Header,
    /// a running footer or page number
    Footer,
}

/// a run of operators drawing one thing: a shown string, a picture, a
/// painted path. `start..=end` index the page's top level operators.
#[derive(Clone, Debug)]
pub(crate) struct Unit {
    pub start: usize,
    pub end: usize,
    pub wrap: Wrap,
}

/// operators a marked-content sequence must not cross.
fn is_boundary(operator: &str) -> bool {
    matches!(
        operator,
        "BT" | "ET" | "q" | "Q" | "BMC" | "BDC" | "EMC" | "BX" | "EX"
    )
}

#[derive(PartialEq)]
enum Open {
    Tag(usize),
    Artifact(ArtifactKind),
}

/// the operators with each unit wrapped as asked, and which structure
/// leaf each new mcid belongs to. mcids are numbered from `next_mcid`.
///
/// with `strip_mcids`, existing sequences that carry an mcid are removed
/// (their content is kept), for retagging a page from scratch.
pub(crate) fn wrap_units(
    operations: &[Operation],
    units: &[Unit],
    next_mcid: &mut i64,
    strip_mcids: bool,
) -> (Vec<Operation>, Vec<(i64, usize)>) {
    let mut output = Vec::with_capacity(operations.len() + units.len() * 2);
    let mut owners = Vec::new();
    let mut open: Option<Open> = None;
    // for each sequence already in the content: was it removed?
    let mut existing: Vec<bool> = Vec::new();
    let mut units = units.iter().peekable();
    let mut index = 0;
    let close = |open: &mut Option<Open>, output: &mut Vec<Operation>| {
        if open.take().is_some() {
            output.push(Operation::new("EMC", Vec::new()));
        }
    };
    while index < operations.len() {
        // units that overlap an earlier one (should not happen) are skipped
        while units.peek().is_some_and(|unit| unit.start < index) {
            units.next();
        }
        if let Some(unit) = units.next_if(|unit| unit.start == index) {
            let end = unit.end.min(operations.len() - 1);
            let wanted = match &unit.wrap {
                Wrap::Tag { target, .. } => Some(Open::Tag(*target)),
                Wrap::Artifact(kind) => Some(Open::Artifact(*kind)),
            };
            if open != wanted {
                close(&mut open, &mut output);
                match &unit.wrap {
                    Wrap::Tag { target, role } => {
                        output.push(begin_tag(role, *next_mcid));
                        owners.push((*next_mcid, *target));
                        *next_mcid += 1;
                    }
                    Wrap::Artifact(kind) => output.push(begin_artifact(*kind)),
                }
                open = wanted;
            }
            output.extend_from_slice(&operations[index..=end]);
            index = end + 1;
            continue;
        }
        let operation = &operations[index];
        if is_boundary(&operation.operator) {
            close(&mut open, &mut output);
            if strip_mcids {
                match operation.operator.as_str() {
                    "BMC" | "BDC" => {
                        let removed = has_mcid(operation);
                        existing.push(removed);
                        if removed {
                            index += 1;
                            continue;
                        }
                    }
                    "EMC" if existing.pop() == Some(true) => {
                        index += 1;
                        continue;
                    }
                    _ => {}
                }
            }
        }
        output.push(operation.clone());
        index += 1;
    }
    close(&mut open, &mut output);
    (output, owners)
}

fn has_mcid(operation: &Operation) -> bool {
    matches!(
        operation.operands.get(1),
        Some(Object::Dictionary(properties)) if properties.has(b"MCID")
    )
}

fn begin_tag(role: &str, mcid: i64) -> Operation {
    let mut properties = Dictionary::new();
    properties.set("MCID", Object::Integer(mcid));
    Operation::new("BDC", vec![name(role), Object::Dictionary(properties)])
}

fn begin_artifact(kind: ArtifactKind) -> Operation {
    let subtype = match kind {
        ArtifactKind::Layout => return Operation::new("BMC", vec![name("Artifact")]),
        ArtifactKind::Header => "Header",
        ArtifactKind::Footer => "Footer",
    };
    let mut properties = Dictionary::new();
    properties.set("Type", name("Pagination"));
    properties.set("Subtype", name(subtype));
    Operation::new(
        "BDC",
        vec![name("Artifact"), Object::Dictionary(properties)],
    )
}

/// turns the sequences carrying these mcids into artifacts, in place, so
/// every operator keeps its index.
pub(crate) fn mcids_to_artifacts(operations: &mut [Operation], mcids: &HashSet<i64>) -> usize {
    let mut changed = 0;
    for operation in operations.iter_mut().filter(|op| op.operator == "BDC") {
        let mcid = match operation.operands.get(1) {
            Some(Object::Dictionary(properties)) => {
                properties.get(b"MCID").ok().and_then(|m| m.as_i64().ok())
            }
            _ => None,
        };
        if mcid.is_some_and(|mcid| mcids.contains(&mcid)) {
            *operation = Operation::new("BMC", vec![name("Artifact")]);
            changed += 1;
        }
    }
    changed
}

/// sets the fill colour to `rgb` just for the operators at `indexes`, and
/// back to what it was right after each, so nothing else changes colour.
pub(crate) fn recolour(
    operations: &[Operation],
    targets: &[(usize, Vec<Operation>)],
    rgb: [f32; 3],
) -> Vec<Operation> {
    let mut output = Vec::with_capacity(operations.len() + targets.len() * 3);
    for (index, operation) in operations.iter().enumerate() {
        match targets.iter().find(|(target, _)| *target == index) {
            Some((_, restore)) => {
                output.push(Operation::new(
                    "rg",
                    rgb.iter().map(|&value| Object::Real(value)).collect(),
                ));
                output.push(operation.clone());
                output.extend(restore.iter().cloned());
            }
            None => output.push(operation.clone()),
        }
    }
    output
}

/// replaces a page's content with new operators in one new stream. the
/// old streams are dropped when no other page uses them.
pub(crate) fn replace_content(
    doc: &mut Document,
    page: ObjectId,
    operations: Vec<Operation>,
) -> Result<(), A11yError> {
    let bytes = Content { operations }
        .encode()
        .map_err(|error| A11yError::Internal(error.to_string()))?;
    let mut stream = Stream::new(Dictionary::new(), bytes);
    // compression only saves space; a failure leaves the stream readable
    let _ = stream.compress();
    let old = doc.get_page_contents(page);
    let new = doc.add_object(stream);
    dict_mut(doc, page)?.set("Contents", Object::Reference(new));
    let still_used: HashSet<ObjectId> = page_ids(doc)
        .into_iter()
        .filter(|&other| other != page)
        .flat_map(|other| doc.get_page_contents(other))
        .collect();
    for id in old {
        if !still_used.contains(&id) {
            doc.objects.remove(&id);
        }
    }
    Ok(())
}

/// the decoded top level operators of a page, as the content reader saw them.
pub(crate) fn page_operations(
    doc: &Document,
    page: ObjectId,
    page_number: u32,
) -> Result<Vec<Operation>, A11yError> {
    Content::decode(&doc.get_page_content(page))
        .map(|content| content.operations)
        .map_err(|_| A11yError::UnreadableContent(page_number + 1))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ops(text: &str) -> Vec<Operation> {
        Content::decode(text.as_bytes()).unwrap().operations
    }

    fn encode(operations: Vec<Operation>) -> String {
        String::from_utf8(Content { operations }.encode().unwrap()).unwrap()
    }

    fn tag(start: usize, target: usize) -> Unit {
        Unit {
            start,
            end: start,
            wrap: Wrap::Tag {
                target,
                role: "P".into(),
            },
        }
    }

    #[test]
    fn text_in_two_text_objects_gets_two_mcids_for_one_tag() {
        let operations = ops("BT (a) Tj ET BT (b) Tj ET");
        let mut next = 0;
        let (output, owners) = wrap_units(&operations, &[tag(1, 7), tag(4, 7)], &mut next, false);
        assert_eq!(
            encode(output),
            "BT\n/P <</MCID 0>> BDC\n(a) Tj\nEMC\nET\nBT\n/P <</MCID 1>> BDC\n(b) Tj\nEMC\nET"
        );
        assert_eq!(owners, vec![(0, 7), (1, 7)]);
    }

    #[test]
    fn neighbouring_strings_of_one_tag_share_a_sequence() {
        let operations = ops("BT (a) Tj 0 -12 Td (b) Tj ET");
        let mut next = 5;
        let (output, owners) = wrap_units(&operations, &[tag(1, 0), tag(3, 0)], &mut next, false);
        assert_eq!(
            encode(output),
            "BT\n/P <</MCID 5>> BDC\n(a) Tj\n0 -12 Td\n(b) Tj\nEMC\nET"
        );
        assert_eq!(owners, vec![(5, 0)]);
    }

    #[test]
    fn old_mcids_are_removed_when_asked() {
        let operations = ops("/Span <</MCID 3>> BDC BT (a) Tj ET EMC");
        let mut next = 0;
        let (output, _) = wrap_units(&operations, &[tag(2, 0)], &mut next, true);
        assert_eq!(encode(output), "BT\n/P <</MCID 0>> BDC\n(a) Tj\nEMC\nET");
    }

    #[test]
    fn tagged_content_can_become_an_artifact() {
        let mut operations = ops("/Figure <</MCID 2>> BDC 0 0 m EMC");
        let changed = mcids_to_artifacts(&mut operations, &[2].into_iter().collect());
        assert_eq!(changed, 1);
        assert_eq!(encode(operations), "/Artifact BMC\n0 0 m\nEMC");
    }
}

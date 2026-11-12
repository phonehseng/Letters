//! the fix wizard's requests, and running each one with the engine.
//!
//! the window sends one `FixRequest` per fix the person confirms. the
//! shell runs it inside `state.edit`, so every fix is one undo step and
//! the file on disk changes only when the person saves. the request
//! carries exactly what the finding's `FixKind` said the fix needs, plus
//! the person's answer (a language, a title, alt text, an order...).

use letters_a11y::{
    self as a11y, A11yError, AutoTagOptions, ContentRef, FixResult, ObjectRef, RegionRenderer, Rgb,
    TableHeaders, TagCounts, TagDecision, Target,
};
use lopdf::Document;
use serde::{Deserialize, Serialize};

/// one heading's new level, from the wizard's editable ladder.
#[derive(Clone, Copy, Debug, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HeadingLevel {
    pub element: ObjectRef,
    pub level: u8,
}

/// a fix the person asked for, with their answer.
#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum FixRequest {
    /// builds tags for an untagged document from its layout
    AddTags {
        language: Option<String>,
        #[serde(default)]
        replace_existing: bool,
    },
    MarkAsTagged,
    SetLanguage {
        language: String,
    },
    SetTitle {
        title: String,
    },
    ShowTitle,
    SetAltText {
        target: Target,
        text: String,
    },
    /// "decorative": screen readers skip it
    MarkAsArtifact {
        target: Target,
    },
    FixHeadingLevels,
    /// the ladder as the person left it, which may differ from the proposal
    SetHeadingLevels {
        levels: Vec<HeadingLevel>,
    },
    SetTableHeaders {
        table: ObjectRef,
        header_rows: u32,
        header_columns: u32,
    },
    SetReadingOrder {
        page: u32,
        order: Vec<ObjectRef>,
    },
    SetFieldTooltip {
        field: ObjectRef,
        text: String,
    },
    SetTabOrder,
    AddLinkText {
        annotation: ObjectRef,
        text: String,
    },
    SetAnnotationContents {
        annotation: ObjectRef,
        text: String,
    },
    TagAnnotations,
    TagUntaggedContent {
        page: u32,
    },
    ChangeTextColour {
        runs: Vec<ContentRef>,
        colour: Rgb,
    },
    AddBookmarksFromHeadings,
    /// writes the pdf/ua identifier; refused unless every must-fix is gone
    MarkPdfua,
}

/// what a fix did, for the wizard to say and (after tagging) to review.
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FixOutcome {
    pub changes: Vec<String>,
    /// the tagger's uncertain decisions, for the person to look over
    pub decisions: Vec<TagDecision>,
    pub counts: Option<TagCounts>,
}

impl From<FixResult> for FixOutcome {
    fn from(result: FixResult) -> Self {
        Self {
            changes: result.changes,
            ..Self::default()
        }
    }
}

impl FixRequest {
    /// true for the fixes that need pictures of the pages (only contrast, for pdf/ua).
    pub fn needs_renderer(&self) -> bool {
        matches!(self, FixRequest::MarkPdfua)
    }
}

/// runs one fix on the document. `renderer` is used only by Mark as PDF/UA,
/// which checks the document again (contrast included) before marking it.
pub fn apply_fix(
    doc: &mut Document,
    request: &FixRequest,
    renderer: Option<&mut dyn RegionRenderer>,
) -> Result<FixOutcome, A11yError> {
    let done: FixResult = match request {
        FixRequest::AddTags {
            language,
            replace_existing,
        } => return add_tags(doc, language.clone(), *replace_existing),
        FixRequest::MarkAsTagged => a11y::mark_as_tagged(doc)?,
        FixRequest::SetLanguage { language } => a11y::set_language(doc, language)?,
        FixRequest::SetTitle { title } => a11y::set_title(doc, title)?,
        FixRequest::ShowTitle => a11y::show_title(doc)?,
        FixRequest::SetAltText { target, text } => a11y::set_alt_text(doc, *target, text)?,
        FixRequest::MarkAsArtifact { target } => a11y::mark_as_artifact(doc, *target)?,
        FixRequest::FixHeadingLevels => a11y::fix_heading_levels(doc)?,
        FixRequest::SetHeadingLevels { levels } => set_heading_levels(doc, levels)?,
        FixRequest::SetTableHeaders {
            table,
            header_rows,
            header_columns,
        } => a11y::set_table_headers(
            doc,
            *table,
            TableHeaders {
                header_rows: *header_rows,
                header_columns: *header_columns,
            },
        )?,
        FixRequest::SetReadingOrder { page, order } => a11y::set_reading_order(doc, *page, order)?,
        FixRequest::SetFieldTooltip { field, text } => a11y::set_field_tooltip(doc, *field, text)?,
        FixRequest::SetTabOrder => a11y::set_tab_order(doc)?,
        FixRequest::AddLinkText { annotation, text } => {
            a11y::add_link_text(doc, *annotation, text)?
        }
        FixRequest::SetAnnotationContents { annotation, text } => {
            a11y::set_annotation_contents(doc, *annotation, text)?
        }
        FixRequest::TagAnnotations => a11y::tag_annotations(doc)?,
        FixRequest::TagUntaggedContent { page } => {
            return a11y::tag_untagged_content(doc, *page).map(tagging_outcome)
        }
        FixRequest::ChangeTextColour { runs, colour } => {
            a11y::apply_text_colour(doc, runs, *colour)?
        }
        FixRequest::AddBookmarksFromHeadings => a11y::add_bookmarks_from_headings(doc)?,
        FixRequest::MarkPdfua => a11y::mark_pdfua(doc, renderer)?,
    };
    Ok(done.into())
}

fn add_tags(
    doc: &mut Document,
    language: Option<String>,
    replace_existing: bool,
) -> Result<FixOutcome, A11yError> {
    let language = language.filter(|tag| !tag.trim().is_empty());
    let options = AutoTagOptions {
        replace_existing,
        language,
    };
    a11y::auto_tag(doc, &options).map(tagging_outcome)
}

fn tagging_outcome(result: a11y::AutoTagResult) -> FixOutcome {
    FixOutcome {
        changes: result.changes,
        decisions: result.decisions,
        counts: Some(result.counts),
    }
}

/// every heading the person changed, in one edit, so one undo takes the
/// whole ladder back.
fn set_heading_levels(doc: &mut Document, levels: &[HeadingLevel]) -> Result<FixResult, A11yError> {
    if levels.is_empty() {
        return Err(A11yError::InvalidInput(
            "There are no heading levels to change.".into(),
        ));
    }
    let mut result = FixResult::default();
    for heading in levels {
        for change in a11y::set_heading_level(doc, heading.element, heading.level)?.changes {
            result.push(change);
        }
    }
    Ok(result)
}

//! every rule the checker knows, with the plain-language text the ui shows.
//!
//! keeping all the wording in one table means the explanations stay
//! consistent, the readme's rule table can be checked against it, and the
//! ui can show "what this checks" for rules that found nothing.

use serde::Serialize;

use crate::report::Severity;

/// how a rule is decided.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Checked {
    /// a program can decide it
    Machine,
    /// only a person can decide it
    ByHand,
}

/// how much of the fixing Letters can do.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum FixMode {
    /// one click, no questions
    Automatic,
    /// Letters needs one answer, such as the alt text or the language
    OneAnswer,
    /// Letters proposes something the person reviews (tags, reading order)
    Proposal,
    /// has to be fixed in the program that made the file, or by hand
    Manual,
}

#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RuleInfo {
    pub id: &'static str,
    pub title: &'static str,
    pub severity: Severity,
    pub checked: Checked,
    pub fix_mode: FixMode,
    /// what the rule is about
    pub explanation: &'static str,
    /// why it matters to a person using a screen reader
    pub why: &'static str,
    /// what the fix does to the file, or how to fix it by hand
    pub fix: &'static str,
    pub pdfua: &'static str,
    /// what the check can't see, said honestly
    pub limits: &'static str,
}

use Checked::{ByHand, Machine};
use FixMode::{Automatic, Manual, OneAnswer, Proposal};
use Severity::{CheckByHand, MustFix, ShouldFix};

pub const RULES: &[RuleInfo] = &[
    // ---- tags ----------------------------------------------------------------
    RuleInfo {
        id: "not-tagged",
        title: "The document has no tags",
        severity: MustFix,
        checked: Machine,
        fix_mode: Proposal,
        explanation: "Tags are a hidden outline of the document that says what each part is: this is a heading, this is a paragraph, this is a picture. This document has none.",
        why: "Without tags a screen reader has to guess the order and meaning of everything on the page. Headings, lists, tables and pictures all come out as one stream of text, or as nothing at all.",
        fix: "Add tags builds a structure tree from the page layout: paragraphs, headings by text size, lists, pictures and simple tables, and marks page numbers and repeating headers as decoration. It is a best guess, so Letters shows what it was unsure about for you to review.",
        pdfua: "PDF/UA-1 §7.1 (Matterhorn 01)",
        limits: "Once a document has no tags, the other tag rules are not checked; they are covered by this one finding.",
    },
    RuleInfo {
        id: "tagged-flag-missing",
        title: "The document isn’t marked as tagged",
        severity: MustFix,
        checked: Machine,
        fix_mode: Automatic,
        explanation: "The document has tags, but the setting that says “this file is tagged” (MarkInfo /Marked) is off.",
        why: "Some screen readers and PDF readers look at that setting first and ignore the tags when it is off.",
        fix: "Turns the setting on. Nothing else changes.",
        pdfua: "PDF/UA-1 §6.2 (Matterhorn 06-002)",
        limits: "",
    },
    RuleInfo {
        id: "structure-tree-damaged",
        title: "The tags are damaged",
        severity: ShouldFix,
        checked: Machine,
        fix_mode: Manual,
        explanation: "Part of the tags points to something that doesn’t exist, or loops back on itself.",
        why: "A screen reader may skip the damaged part, read it twice, or stop reading the document.",
        fix: "Fix it in the program that made the PDF and export again, or remove the tags and use Add tags to build new ones.",
        pdfua: "PDF/UA-1 §7.1",
        limits: "Letters reads past the damage, so the other findings still cover the parts it could read.",
    },
    // ---- language ------------------------------------------------------------
    RuleInfo {
        id: "missing-language",
        title: "The document’s language isn’t set",
        severity: MustFix,
        checked: Machine,
        fix_mode: OneAnswer,
        explanation: "The document doesn’t say what language it is written in.",
        why: "A screen reader picks its voice and pronunciation from the language. Without it, English text may be read with French pronunciation, or the other way round.",
        fix: "Stores the language you choose (like en-US) in the document.",
        pdfua: "PDF/UA-1 §7.2 (Matterhorn 11-006)",
        limits: "",
    },
    RuleInfo {
        id: "invalid-language",
        title: "A language code isn’t valid",
        severity: MustFix,
        checked: Machine,
        fix_mode: OneAnswer,
        explanation: "A language is set, but the code isn’t one screen readers understand. Codes look like en, en-US, fr or zh-Hans.",
        why: "A screen reader that can’t read the code falls back to its default voice, which may be the wrong language.",
        fix: "Replaces the code with the one you choose.",
        pdfua: "PDF/UA-1 §7.2 (Matterhorn 11-001 to 11-005)",
        limits: "Codes are checked against the shape of BCP 47 tags, not against the full list of registered languages.",
    },
    RuleInfo {
        id: "language-change-unmarked",
        title: "Text in another language isn’t marked",
        severity: ShouldFix,
        checked: Machine,
        fix_mode: Manual,
        explanation: "Some text is written in a different alphabet from the document’s language (for example Greek or Chinese inside an English document), and its tags don’t say which language it is.",
        why: "A screen reader reads it with the document’s voice, which mispronounces it or skips it.",
        fix: "Select the text’s tag and set its language in the tags panel.",
        pdfua: "PDF/UA-1 §7.2 (Matterhorn 11-001)",
        limits: "Only changes of alphabet are found. A French quotation in an English document uses the same letters, so it can only be found by hand.",
    },
    // ---- title -----------------------------------------------------------------
    RuleInfo {
        id: "missing-title",
        title: "The document has no title",
        severity: MustFix,
        checked: Machine,
        fix_mode: OneAnswer,
        explanation: "The document’s information doesn’t include a title, or has it only in the old information dictionary and not in the XMP metadata.",
        why: "The title is the first thing a screen reader announces. Without it, people hear the file name, which is often something like “final_v3_FINAL.pdf”.",
        fix: "Stores the title in the document information and the XMP metadata, and makes readers show it.",
        pdfua: "PDF/UA-1 §7.1 (Matterhorn 06-003)",
        limits: "",
    },
    RuleInfo {
        id: "title-not-displayed",
        title: "Readers show the file name instead of the title",
        severity: MustFix,
        checked: Machine,
        fix_mode: Automatic,
        explanation: "The setting that tells PDF readers to show the title in the window (DisplayDocTitle) is off.",
        why: "The window title is what a screen reader announces when you switch to the document.",
        fix: "Turns the setting on.",
        pdfua: "PDF/UA-1 §7.1 (Matterhorn 07-001)",
        limits: "",
    },
    // ---- pictures --------------------------------------------------------------
    RuleInfo {
        id: "missing-alt-text",
        title: "A picture has no alt text",
        severity: MustFix,
        checked: Machine,
        fix_mode: OneAnswer,
        explanation: "Alt text is a short written description of a picture, read aloud in its place. This picture has none.",
        why: "A screen reader can only say “image”, or nothing, so the person misses whatever the picture shows.",
        fix: "Stores the description you write as the picture’s alt text. If the picture is only decoration, mark it as decoration instead and screen readers will skip it.",
        pdfua: "PDF/UA-1 §7.3 (Matterhorn 13-004)",
        limits: "",
    },
    RuleInfo {
        id: "alt-text-is-file-name",
        title: "A picture’s alt text is a file name or placeholder",
        severity: MustFix,
        checked: Machine,
        fix_mode: OneAnswer,
        explanation: "The alt text looks like a file name (photo.jpg) or a placeholder (“image”, “picture 3”), not a description.",
        why: "Hearing “IMG underscore 2041 dot JPEG” tells the person nothing about the picture.",
        fix: "Replaces it with the description you write.",
        pdfua: "PDF/UA-1 §7.3 (Matterhorn 13-004)",
        limits: "Only obvious file names and placeholders are caught; whether alt text is actually helpful is a check by hand.",
    },
    RuleInfo {
        id: "image-not-tagged",
        title: "A picture isn’t in the tags",
        severity: MustFix,
        checked: Machine,
        fix_mode: OneAnswer,
        explanation: "A picture is drawn on the page but is neither tagged as a figure nor marked as decoration.",
        why: "Screen readers skip untagged pictures without saying anything, so the person doesn’t know they exist.",
        fix: "Wraps the picture in a Figure tag with the alt text you write, or marks it as decoration.",
        pdfua: "PDF/UA-1 §7.1, §7.3 (Matterhorn 01-005)",
        limits: "",
    },
    // ---- headings ----------------------------------------------------------------
    RuleInfo {
        id: "no-headings",
        title: "The document has no headings",
        severity: ShouldFix,
        checked: Machine,
        fix_mode: Manual,
        explanation: "None of the tags are headings (H1 to H6), though the document is long enough to have sections.",
        why: "Screen reader users jump between headings to find their way, the way sighted readers skim bold titles. Without headings they have to listen from the start.",
        fix: "Select the text of each section title and use Set heading level, or tag the document again with Add tags.",
        pdfua: "PDF/UA-1 §7.4 (WCAG 2.4.6)",
        limits: "Only documents of two pages or more, or with more than 300 words, are expected to have headings.",
    },
    RuleInfo {
        id: "first-heading-not-h1",
        title: "The first heading isn’t level 1",
        severity: MustFix,
        checked: Machine,
        fix_mode: Automatic,
        explanation: "The headings are numbered (H1 to H6), but the first one isn’t an H1.",
        why: "Screen readers announce heading levels. Starting at level 2 or 3 suggests the start of the document is missing.",
        fix: "Renumbers the headings so the first is level 1 and no level is skipped, keeping their relative order.",
        pdfua: "PDF/UA-1 §7.4.2 (Matterhorn 14-002)",
        limits: "",
    },
    RuleInfo {
        id: "heading-level-skipped",
        title: "A heading level is skipped",
        severity: MustFix,
        checked: Machine,
        fix_mode: Automatic,
        explanation: "A heading jumps more than one level deeper than the one before, like an H1 followed by an H3.",
        why: "People navigating by headings use the levels as a table of contents. A jump makes it look as if a section is missing.",
        fix: "Renumbers the headings so no level is skipped, keeping their relative order.",
        pdfua: "PDF/UA-1 §7.4.2 (Matterhorn 14-003)",
        limits: "",
    },
    // ---- reading order -------------------------------------------------------------
    RuleInfo {
        id: "reading-order-mismatch",
        title: "The reading order doesn’t follow the page",
        severity: ShouldFix,
        checked: Machine,
        fix_mode: Proposal,
        explanation: "Reading order is the order in which a screen reader reads the parts of a page. On this page the tags’ order is very different from top-to-bottom, column-by-column order.",
        why: "A screen reader follows the tags, not the layout, so the person may hear the end of a column before its start, or a caption far from its picture.",
        fix: "Letters proposes an order from the layout (top to bottom, one column at a time). You can adjust it before it rewrites the order of the tags.",
        pdfua: "PDF/UA-1 §7.2 (Matterhorn 09-001)",
        limits: "Only strong disagreements are reported. Layouts like sidebars or pull quotes can be right in an unusual order, so whether the order is logical is also a check by hand.",
    },
    // ---- tables ------------------------------------------------------------------------
    RuleInfo {
        id: "table-missing-headers",
        title: "A table has no header cells",
        severity: MustFix,
        checked: Machine,
        fix_mode: OneAnswer,
        explanation: "None of the table’s cells are tagged as headers (TH).",
        why: "When moving through a table, a screen reader announces each cell’s column and row headers. Without them the person hears a list of numbers with no idea what they mean.",
        fix: "Turns the rows and columns you choose (usually the first row) into header cells and says whether each heads a column or a row.",
        pdfua: "PDF/UA-1 §7.5 (Matterhorn 15-005)",
        limits: "",
    },
    RuleInfo {
        id: "table-header-missing-scope",
        title: "A table header doesn’t say what it heads",
        severity: MustFix,
        checked: Machine,
        fix_mode: OneAnswer,
        explanation: "A header cell (TH) doesn’t say whether it heads a column or a row (its Scope), and the table doesn’t link cells to headers another way.",
        why: "Without a scope, screen readers have to guess which cells a header belongs to, and in larger tables they often guess wrong.",
        fix: "Sets the scope of the header cells: column for the header rows, row for the header columns.",
        pdfua: "PDF/UA-1 §7.5 (Matterhorn 15-003)",
        limits: "",
    },
    RuleInfo {
        id: "table-irregular-rows",
        title: "A table’s rows have different lengths",
        severity: ShouldFix,
        checked: Machine,
        fix_mode: Manual,
        explanation: "Counting merged cells, some rows of this table have more cells than others.",
        why: "Screen readers work out which header goes with which cell by position. Uneven rows shift the headers, so cells are announced with the wrong ones.",
        fix: "Tag merged cells with their row and column span, or add the missing empty cells, in the program that made the PDF.",
        pdfua: "PDF/UA-1 §7.5 (Matterhorn 15-004)",
        limits: "",
    },
    RuleInfo {
        id: "table-structure-invalid",
        title: "A table’s tags are in the wrong shape",
        severity: MustFix,
        checked: Machine,
        fix_mode: Manual,
        explanation: "A table’s tags aren’t nested the way tables must be: a Table holds rows (TR), and rows hold cells (TH or TD).",
        why: "Screen readers only offer table navigation when the tags have the table shape; otherwise the cells come out as loose text.",
        fix: "Fix the table in the program that made the PDF, or remove its tags and tag it again.",
        pdfua: "PDF/UA-1 §7.2 (Matterhorn 09-004)",
        limits: "",
    },
    // ---- lists ------------------------------------------------------------------------
    RuleInfo {
        id: "list-structure-invalid",
        title: "A list’s tags are in the wrong shape",
        severity: MustFix,
        checked: Machine,
        fix_mode: Manual,
        explanation: "A list’s tags aren’t nested the way lists must be: a list (L) holds items (LI), and each item holds its label (Lbl, like the bullet) and its body (LBody).",
        why: "With the right tags a screen reader says “list of 5 items” and lets the person skip it; otherwise the items are just loose lines.",
        fix: "Fix the list in the program that made the PDF, or remove its tags and tag it again.",
        pdfua: "PDF/UA-1 §7.6 (Matterhorn 16-001, 16-002)",
        limits: "",
    },
    RuleInfo {
        id: "list-not-tagged-as-list",
        title: "Something that looks like a list isn’t tagged as one",
        severity: ShouldFix,
        checked: Machine,
        fix_mode: Manual,
        explanation: "Two or more paragraphs in a row start with a bullet or a number, but they are tagged as paragraphs, not as a list.",
        why: "A screen reader can’t say how many items there are or let the person skip past the list.",
        fix: "Tag the paragraphs as a list in the tags panel, or tag the document again with Add tags.",
        pdfua: "PDF/UA-1 §7.6 (WCAG 1.3.1)",
        limits: "Only lines starting with common bullets (•, –, *) or numbers (1. or a)) are recognised.",
    },
    // ---- links, notes and forms ---------------------------------------------------------
    RuleInfo {
        id: "link-not-tagged",
        title: "A link isn’t in the tags",
        severity: MustFix,
        checked: Machine,
        fix_mode: Automatic,
        explanation: "A clickable link area isn’t connected to a Link tag.",
        why: "Screen readers list and announce links from the tags. An untagged link can’t be found by moving through the text.",
        fix: "Adds a Link tag for the link, inside the tag of the text it sits on.",
        pdfua: "PDF/UA-1 §7.18.5 (Matterhorn 28-011)",
        limits: "",
    },
    RuleInfo {
        id: "link-missing-description",
        title: "A link has no description",
        severity: MustFix,
        checked: Machine,
        fix_mode: OneAnswer,
        explanation: "The link doesn’t carry a short description of where it goes (its Contents).",
        why: "Screen readers announce this description when the person tabs to the link; without it they may only hear the web address or nothing.",
        fix: "Stores the description you write (or the suggested one from the link’s text) with the link.",
        pdfua: "PDF/UA-1 §7.18.5 (Matterhorn 28-004)",
        limits: "",
    },
    RuleInfo {
        id: "link-text-not-descriptive",
        title: "A link’s text doesn’t say where it goes",
        severity: ShouldFix,
        checked: Machine,
        fix_mode: OneAnswer,
        explanation: "The text of the link is something like “click here”, “more” or a bare web address.",
        why: "Screen reader users often list all links on a page. A list of “click here, click here, here” is useless.",
        fix: "Stores a clearer description with the link. The words on the page stay the same, so also consider rewording them in the original.",
        pdfua: "WCAG 2.4.4",
        limits: "Only common vague phrases and bare addresses are caught.",
    },
    RuleInfo {
        id: "annotation-not-tagged",
        title: "A comment or note isn’t in the tags",
        severity: MustFix,
        checked: Machine,
        fix_mode: Automatic,
        explanation: "A comment, note or other annotation isn’t connected to an Annot tag.",
        why: "Screen readers can’t place an untagged comment in the reading order, so it is read out of place or not at all.",
        fix: "Adds an Annot tag for it, next to the content it sits on.",
        pdfua: "PDF/UA-1 §7.18.1 (Matterhorn 28-002)",
        limits: "",
    },
    RuleInfo {
        id: "annotation-missing-contents",
        title: "A comment or note has no text",
        severity: MustFix,
        checked: Machine,
        fix_mode: OneAnswer,
        explanation: "An annotation (a stamp, a highlight, a drawing) has no text description.",
        why: "A screen reader announces “annotation” and nothing else.",
        fix: "Stores the description you write with the annotation.",
        pdfua: "PDF/UA-1 §7.18.1 (Matterhorn 28-004)",
        limits: "",
    },
    RuleInfo {
        id: "field-missing-tooltip",
        title: "A form field has no name people can hear",
        severity: MustFix,
        checked: Machine,
        fix_mode: OneAnswer,
        explanation: "A form field has no tooltip (TU), the name a screen reader reads out for it.",
        why: "Tabbing into the field, the person hears “edit text” with no idea what to type.",
        fix: "Stores the name you write (suggested from the field’s internal name) as its tooltip.",
        pdfua: "PDF/UA-1 §7.18.4 (Matterhorn 28-005)",
        limits: "",
    },
    RuleInfo {
        id: "field-not-tagged",
        title: "A form field isn’t in the tags",
        severity: MustFix,
        checked: Machine,
        fix_mode: Automatic,
        explanation: "A form field isn’t connected to a Form tag.",
        why: "Screen readers can’t place the field in the reading order next to its label.",
        fix: "Adds a Form tag for the field.",
        pdfua: "PDF/UA-1 §7.18.4 (Matterhorn 28-011)",
        limits: "",
    },
    RuleInfo {
        id: "tab-order-not-structure",
        title: "Tab order doesn’t follow the tags",
        severity: MustFix,
        checked: Machine,
        fix_mode: Automatic,
        explanation: "A page with links, comments or fields doesn’t say that the Tab key should move through them in tag order.",
        why: "Keyboard and screen reader users then jump between fields in an order that may not match the page.",
        fix: "Sets every page with links or fields to follow the tags’ order (/Tabs /S).",
        pdfua: "PDF/UA-1 §7.18.3 (Matterhorn 28-010)",
        limits: "",
    },
    // ---- content ---------------------------------------------------------------------------
    RuleInfo {
        id: "untagged-content",
        title: "Some content isn’t tagged",
        severity: MustFix,
        checked: Machine,
        fix_mode: Proposal,
        explanation: "Text or shapes on this page are neither in the tags nor marked as decoration.",
        why: "Screen readers skip untagged content. If it is real content, the person never hears it; if it is decoration, it should say so.",
        fix: "Tags the untagged text as paragraphs and pictures as figures at the end of the page’s tags, and marks lines and shapes as decoration. Review the result.",
        pdfua: "PDF/UA-1 §7.1 (Matterhorn 01-005)",
        limits: "",
    },
    // ---- fonts ------------------------------------------------------------------------------
    RuleInfo {
        id: "font-not-embedded",
        title: "A font isn’t embedded",
        severity: MustFix,
        checked: Machine,
        fix_mode: Manual,
        explanation: "A font used for visible text isn’t stored in the file, so each reader substitutes its own.",
        why: "Substituted fonts can change letter widths and lose characters, and PDF/UA requires every font to be embedded so the text is the same everywhere.",
        fix: "Export the PDF again with “embed fonts” turned on in the program that made it.",
        pdfua: "PDF/UA-1 §7.21.4.1 (Matterhorn 31-001)",
        limits: "Fonts used only for invisible text (such as the text layer of a scan) are not required to be embedded.",
    },
    RuleInfo {
        id: "text-not-unicode",
        title: "Some text can’t be turned into real characters",
        severity: MustFix,
        checked: Machine,
        fix_mode: Manual,
        explanation: "A font has no map from its codes to real characters (ToUnicode), and its encoding isn’t a standard one.",
        why: "A screen reader gets nonsense or silence instead of the words, and copying or searching the text fails too.",
        fix: "Export the PDF again from the original, or run text recognition on these pages.",
        pdfua: "PDF/UA-1 §7.21.7 (Matterhorn 31-006)",
        limits: "Encodings with unusual but standard glyph names may be reported; check a copy-paste of the text if unsure.",
    },
    // ---- contrast ----------------------------------------------------------------------------
    RuleInfo {
        id: "low-contrast",
        title: "Text is hard to see against its background",
        severity: ShouldFix,
        checked: Machine,
        fix_mode: OneAnswer,
        explanation: "The colour of some text is too close to the colour behind it. Normal text needs a contrast ratio of at least 4.5 to 1; large text (18 pt, or 14 pt bold) needs 3 to 1.",
        why: "People with low vision or colour blindness, and anyone reading in sunlight, may not be able to read it. Screen readers don’t care, but many people who use magnifiers do.",
        fix: "Changes the colour of just that text to the nearest colour that passes, keeping its hue. Other text and the background stay as they are.",
        pdfua: "WCAG 2.1 SC 1.4.3 (PDF/UA leaves contrast to WCAG)",
        limits: "Needs the page picture to measure the background; without it this rule is not checked. Text over photos or gradients is measured against the most common colour behind it.",
    },
    // ---- the document as a whole --------------------------------------------------------------
    RuleInfo {
        id: "no-bookmarks",
        title: "A long document has no bookmarks",
        severity: ShouldFix,
        checked: Machine,
        fix_mode: Automatic,
        explanation: "The document is long but has no bookmarks (the clickable table of contents in the sidebar).",
        why: "Bookmarks let everyone, including keyboard and screen reader users, jump straight to a section.",
        fix: "Adds bookmarks made from the document’s headings.",
        pdfua: "WCAG 2.4.5 (PDF/UA-1 §7.1 recommends them)",
        limits: "Only documents above the page threshold (10 pages by default) are expected to have bookmarks. The automatic fix needs headings in the tags.",
    },
    RuleInfo {
        id: "page-labels-invalid",
        title: "The page numbers shown by readers are inconsistent",
        severity: ShouldFix,
        checked: Machine,
        fix_mode: Manual,
        explanation: "The page labels (the page numbers a PDF reader shows, like i, ii, 1, 2) are out of order or point past the last page.",
        why: "A screen reader announces “page iv” or “page 12”; if those don’t match the printed page numbers, references like “see page 12” lead to the wrong place.",
        fix: "Set the page labels again in the page tools, or in the program that made the PDF.",
        pdfua: "WCAG 2.4.5",
        limits: "",
    },
    RuleInfo {
        id: "security-blocks-assistive-tech",
        title: "The security settings block screen readers",
        severity: MustFix,
        checked: Machine,
        fix_mode: Manual,
        explanation: "The document’s security settings don’t allow accessibility tools to read its text.",
        why: "Some screen readers obey the setting and refuse to read the document at all.",
        fix: "Ask the owner of the document to allow “content copying for accessibility”, or remove the security in the security tool if you are the owner.",
        pdfua: "PDF/UA-1 §7.16 (Matterhorn 26-002)",
        limits: "",
    },
    RuleInfo {
        id: "image-only-page",
        title: "A page is only a picture of text",
        severity: MustFix,
        checked: Machine,
        fix_mode: Manual,
        explanation: "This page is a scanned image with no real text behind it.",
        why: "A screen reader finds nothing to read: to it the page is blank. Searching and copying don’t work either.",
        fix: "Run text recognition (OCR) on these pages to add real text, then tag the document.",
        pdfua: "PDF/UA-1 §7.1, §7.3",
        limits: "A page counts as scanned when pictures cover at least half of it and there is no text at all.",
    },
    RuleInfo {
        id: "scripted-or-moving-content",
        title: "Check scripts and moving content by hand",
        severity: CheckByHand,
        checked: ByHand,
        fix_mode: Manual,
        explanation: "The document contains JavaScript, video, sound or other media.",
        why: "Content that only works with scripts may be unreachable for screen readers, and anything that flashes more than three times a second can cause seizures.",
        fix: "Check that everything also works without scripts and that nothing flashes. Letters can’t run or watch it for you.",
        pdfua: "PDF/UA-1 §7.1 (Matterhorn 02, WCAG 2.3.1)",
        limits: "Letters only reports that such content exists; it does not run it.",
    },
    // ---- checks only a person can make -----------------------------------------------------------
    RuleInfo {
        id: "check-alt-text-meaningful",
        title: "Check that alt text is meaningful",
        severity: CheckByHand,
        checked: ByHand,
        fix_mode: Manual,
        explanation: "Letters can see that pictures have alt text, but not whether it describes them well.",
        why: "Good alt text says what the picture shows and why it is there, in a sentence or two. “Chart” or a repeat of the caption doesn’t help.",
        fix: "Read each picture’s alt text next to the picture and improve any that don’t say what matters.",
        pdfua: "PDF/UA-1 §7.3 (Matterhorn 13-005)",
        limits: "",
    },
    RuleInfo {
        id: "check-reading-order-logical",
        title: "Check that the reading order makes sense",
        severity: CheckByHand,
        checked: ByHand,
        fix_mode: Manual,
        explanation: "Letters compares the tags’ order with the layout, but only a person can tell whether the order makes sense.",
        why: "Captions, sidebars and footnotes need to be read at the right moment, which depends on meaning, not position.",
        fix: "Open the reading order view and step through each page.",
        pdfua: "PDF/UA-1 §7.2 (Matterhorn 09-001)",
        limits: "",
    },
    RuleInfo {
        id: "check-headings-for-structure",
        title: "Check that headings mark sections, not looks",
        severity: CheckByHand,
        checked: ByHand,
        fix_mode: Manual,
        explanation: "Headings should be the titles of sections. Text that is only big or bold for emphasis shouldn’t be a heading, and every section title should be one.",
        why: "People move through a document by its headings. Headings used for decoration send them to the wrong places.",
        fix: "Look through the heading list and change the level of any heading that isn’t a section title.",
        pdfua: "PDF/UA-1 §7.4 (Matterhorn 14-001)",
        limits: "",
    },
    RuleInfo {
        id: "check-colour-not-only-meaning",
        title: "Check that colour isn’t the only way meaning is shown",
        severity: CheckByHand,
        checked: ByHand,
        fix_mode: Manual,
        explanation: "Things like “the items in red are due today” or a chart told apart only by colour can’t be understood without seeing colour.",
        why: "Screen readers don’t announce colour, and many people can’t tell some colours apart.",
        fix: "Add a word, symbol or pattern wherever colour carries meaning.",
        pdfua: "PDF/UA-1 §7.1 (Matterhorn 04-001, WCAG 1.4.1)",
        limits: "",
    },
    RuleInfo {
        id: "check-language-changes",
        title: "Check for phrases in another language",
        severity: CheckByHand,
        checked: ByHand,
        fix_mode: Manual,
        explanation: "Quotes or terms in another language that uses the same alphabet (a French phrase in an English essay) can’t be found automatically.",
        why: "The screen reader pronounces them with the document’s voice, which can make them impossible to understand.",
        fix: "Find such phrases and set their language in the tags panel.",
        pdfua: "PDF/UA-1 §7.2 (Matterhorn 11-001)",
        limits: "",
    },
];

/// the catalog entry for a rule id.
pub fn info(id: &str) -> &'static RuleInfo {
    RULES
        .iter()
        .find(|rule| rule.id == id)
        .unwrap_or_else(|| panic!("rule {id} is not in the catalog"))
}

/// rules whose "not checked" means pdf/ua can't be claimed: every
/// machine-checked pdf/ua rule. contrast is a wcag rule, not a pdf/ua
/// one, so a missing renderer doesn't block it.
pub fn blocks_pdfua(id: &str) -> bool {
    RULES.iter().any(|rule| {
        rule.id == id
            && rule.checked == Machine
            && rule.severity == MustFix
            && rule.pdfua.starts_with("PDF/UA")
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rule_ids_are_unique() {
        let mut ids: Vec<&str> = RULES.iter().map(|rule| rule.id).collect();
        ids.sort();
        ids.dedup();
        assert_eq!(ids.len(), RULES.len());
    }

    #[test]
    fn the_readme_lists_every_rule() {
        let readme = include_str!("../../README.md");
        for rule in RULES {
            assert!(
                readme.contains(&format!("| `{}` |", rule.id)),
                "{} is missing from the readme",
                rule.id
            );
        }
    }

    #[test]
    fn every_rule_explains_itself() {
        for rule in RULES {
            assert!(!rule.explanation.is_empty() && !rule.why.is_empty() && !rule.fix.is_empty());
            assert!(!rule.pdfua.is_empty(), "{}", rule.id);
        }
    }
}

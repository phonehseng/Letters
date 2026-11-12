// the words of the Accessibility tab, joined into the one list in
// ../tools.ts, so labels, tooltips and Explain stay in one place.

import type { ToolHelp } from "../tools";

const ONLY_THE_WORKING_COPY = "The file on disk changes only when you choose Save.";

export const a11yTools = {
  checkAccessibility: {
    label: "Check This Document",
    tooltip: "Look for what makes this PDF hard to use with a screen reader",
    whenToUse:
      "Before you hand in or publish a PDF, so people who listen to documents, or who see them differently, can use it too.",
    fileEffect: "Nothing. Checking only reads the document.",
    meaning: "An accessible PDF works for everyone, including people who use a screen reader, which reads the document aloud.",
  },
  fixAccessibility: {
    label: "Fix Problems…",
    tooltip: "Go through the problems Letters can fix, one at a time",
    whenToUse: "After a check, to add what is missing: a language, a title, descriptions of pictures, headings, tags.",
    fileEffect: `Each fix changes your working copy and is one step for Undo. ${ONLY_THE_WORKING_COPY}`,
  },
  addTagsAutomatically: {
    label: "Add Tags Automatically",
    tooltip: "Build tags for this document from its layout",
    whenToUse: "For a PDF with no tags at all, which most screen readers can’t follow. Look over what Letters was unsure about afterwards.",
    fileEffect: `Adds tags to your working copy. The pages look exactly the same. Undo takes the tags off. ${ONLY_THE_WORKING_COPY}`,
  },
  markPdfUa: {
    label: "Mark as PDF/UA",
    tooltip: "Say in the file that it follows the PDF/UA standard",
    whenToUse:
      "Only when every check passes and you have gone through the checks by hand yourself. The mark is a promise to readers and their software.",
    fileEffect: `Writes the PDF/UA mark into your working copy. ${ONLY_THE_WORKING_COPY}`,
  },
  saveAccessibilityReport: {
    label: "Save Report…",
    tooltip: "Save the check’s findings as a web page or plain text",
    whenToUse: "To keep a record of the check, or to send it to whoever made the document.",
    fileEffect: "Writes a new file with the report. The document doesn’t change.",
  },
  showOnPage: {
    label: "Show on Page",
    tooltip: "Go to the page and outline the spot",
    whenToUse: "To see exactly which picture, table or text a finding is about.",
    fileEffect: "Nothing. The outline is only on screen.",
  },
} satisfies Record<string, ToolHelp>;

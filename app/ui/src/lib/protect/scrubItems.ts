// the kinds of hidden information "Remove Hidden Information" lists:
// a name, one sentence about each, and whether it is ticked to start.
//
// what is safe to remove is ticked. comments and form answers are not:
// they are often the point of the file (feedback, a filled-in form), and
// removing them by surprise would lose someone's work.

import type { ScrubFinding, ScrubItem, ScrubReport } from "./protectApi";

export interface ScrubKind {
  label: string;
  explanation: string;
  ticked: boolean;
}

export const SCRUB_KINDS: Record<ScrubItem, ScrubKind> = {
  documentInfo: {
    label: "Author and other properties",
    explanation: "The title, author, subject, keywords, the app that made the file and when.",
    ticked: true,
  },
  xmpMetadata: {
    label: "Metadata",
    explanation: "A second, longer copy of the properties, which can also name people and earlier versions.",
    ticked: true,
  },
  attachments: {
    label: "Attached files",
    explanation: "Files carried inside the PDF, such as the document it was made from.",
    ticked: true,
  },
  javaScript: {
    label: "Scripts and actions",
    explanation: "Code that runs when the file opens, and actions that open apps or send data.",
    ticked: true,
  },
  hiddenLayers: {
    label: "Hidden layers",
    explanation: "Content on layers that are switched off, which other apps can switch on again.",
    ticked: true,
  },
  comments: {
    label: "Comments and marks",
    explanation: "Notes, highlights, drawings and stamps. Links and form fields stay.",
    ticked: false,
  },
  formValues: {
    label: "Form answers",
    explanation: "What was typed or chosen in the form’s fields. The empty fields stay.",
    ticked: false,
  },
  thumbnails: {
    label: "Page thumbnails",
    explanation: "Small pictures of pages some apps store, which can show content that was changed since.",
    ticked: true,
  },
  pieceInfo: {
    label: "App data",
    explanation: "Data the app that made the file kept for itself, which can hold earlier versions.",
    ticked: true,
  },
  privateData: {
    label: "Other private data",
    explanation: "Private data of drawing apps, such as the editable original of a page.",
    ticked: true,
  },
  unusedObjects: {
    label: "Unused content",
    explanation: "Parts nothing in the document shows any more. Saving after this tool drops them anyway.",
    ticked: true,
  },
  externalLinks: {
    label: "Links to websites and other files",
    explanation: "Links that open a web page, another file or an app.",
    ticked: true,
  },
  fingerprints: {
    label: "File identifiers",
    explanation: "The ids that let copies of this file be matched to each other. New ones are made.",
    ticked: true,
  },
};

export interface ScrubRow extends ScrubKind {
  item: ScrubItem;
  count: number;
  details: string[];
}

/** what the file contains, in the order of the engine's list. kinds with nothing found are left out. */
export function scrubRows(report: ScrubReport): ScrubRow[] {
  return report.findings
    .filter((finding: ScrubFinding) => finding.count > 0)
    .map((finding) => ({ ...SCRUB_KINDS[finding.item], item: finding.item, count: finding.count, details: finding.details }));
}

/** the rows ticked at the start. */
export function startingChoice(rows: ScrubRow[]): ScrubItem[] {
  return rows.filter((row) => row.ticked).map((row) => row.item);
}

/** a row's count in words: "3 found". */
export function countText(count: number): string {
  return `${count} found`;
}

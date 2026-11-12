// opening the kinds of file Letters registers for that are neither pdfs
// nor word processor documents: spreadsheets and presentations are shown
// as pdfs that Microsoft Office or LibreOffice makes. (pictures go to New
// from Images, lib/ocr; rtf goes to the writer, lib/writer.)

import { extensionOf, nameOf } from "../writer/routing";

const SHOWN_AS_PDF = ["xlsx", "xls", "ods", "pptx", "ppt", "odp"];

export function isShownAsPdf(path: string): boolean {
  return SHOWN_AS_PDF.includes(extensionOf(path));
}

/** the line shown above such a file once it is open. */
export function shownAsPdfSentence(path: string, program: string): string {
  return `Letters shows “${nameOf(path)}” as a PDF laid out by ${program}. To keep it, choose Save As.`;
}

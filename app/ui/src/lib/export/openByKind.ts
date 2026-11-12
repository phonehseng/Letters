// opening the kinds of file Letters registers for that are neither pdfs
// nor word processor documents: presentations are shown as pdfs that
// Microsoft Office or LibreOffice makes. spreadsheets open in the sheets
// screen (lib/sheets) and are shown as pdfs only when its engine can't
// read them (an old .xls, a damaged file). (pictures go to New
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

/** spreadsheets: the line shown when the sheets screen couldn't read a workbook and Office drew it instead. */
export function sheetShownAsPdfSentence(path: string, program: string, reason: string): string {
  return `Letters couldn’t open “${nameOf(path)}” as a spreadsheet (${reason.replace(/\.$/, "")}), so it shows a PDF laid out by ${program}. To edit it, save it as .xlsx in ${program} first.`;
}

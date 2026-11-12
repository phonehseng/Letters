// the sentences of the Scan tab: progress, the summary after a run, and
// the lines under its buttons. kept together, tested, and written in the
// app's plain style (docs/DESIGN.md).

import type { JobView } from "./ocrJob";
import type { OcrSummary } from "./ocrApi";
import { pagesInWords } from "./pageRange";

/** the engine's own sentence for the word (core/ocr README). */
export const OCR_MEANING =
  "OCR (optical character recognition) reads the letters in a picture of a page, such as a scan or a photo, and turns them into real text you can search, select and copy.";

function plural(count: number, one: string, many = `${one}s`): string {
  return `${count} ${count === 1 ? one : many}`;
}

/** while a run goes: "Reading page 3 of 14…". */
export function progressSentence(view: JobView): string {
  switch (view.phase) {
    case "starting":
      return "Getting ready to read…";
    case "running":
      return view.done >= view.total ? "Finishing…" : `Reading page ${view.done + 1} of ${view.total}…`;
    case "cancelling":
      return "Stopping…";
    case "applying":
      return "Adding the text to the pages…";
    default:
      return "";
  }
}

/** "Text found on 12 of 14 pages; 2 pages look blank." */
export function summarySentence(summary: OcrSummary): string {
  const read = summary.pages.length;
  const found = summary.pagesWithText;
  const blank = summary.blankPages.length;
  if (found === 0) return "No text was found. The pages may be blank, or the scan too faint to read.";
  const where = read === 1 ? `Text found on page ${summary.pages[0] + 1}` : found === read ? `Text found on all ${read} pages` : `Text found on ${found} of ${read} pages`;
  if (blank === 0) return `${where}.`;
  return `${where}; ${plural(blank, "page")} ${blank === 1 ? "looks" : "look"} blank.`;
}

/** how sure the reader was, on average. */
export function confidenceSentence(summary: OcrSummary): string {
  if (summary.wordCount === 0) return "";
  return `On average Letters was ${Math.round(summary.meanConfidence)}% sure of each word.`;
}

/** the pages worth checking by eye. */
export function lowPagesSentence(summary: OcrSummary, pageCount: number): string {
  const pages = summary.lowConfidencePages.map((low) => low.page);
  if (pages.length === 0) return "";
  return `Check ${pagesInWords(pages, pageCount)}: some words there may be wrong. Show Recognized Text marks the doubtful ones.`;
}

/** after a cancel: nothing was added. */
export function cancelledSentence(view: Extract<JobView, { phase: "cancelled" }>): string {
  return view.done === 0
    ? "Stopped before any page was read. Nothing was added."
    : `Stopped after ${plural(view.done, "page")} of ${view.total}. Nothing was added, so the document is as it was.`;
}

/** the line under Recognize Text. */
export function recognizeEffect(pages: number[], pageCount: number, language: string): string {
  if (pages.length === 0) return "Choose the pages to read.";
  if (!language) return "Choose the language the document is written in.";
  return `Reads ${pagesInWords(pages, pageCount)} in ${language} and adds an invisible text layer over the pictures, so Find, selecting and copying work. The pages look the same. One Undo takes it all back; the file on disk changes only when you choose Save.`;
}

/** the line under Remove Recognized Text. */
export function removeEffect(pages: number[], pageCount: number): string {
  if (pages.length === 0) return "No page in this document has text that Letters recognized.";
  return `Takes the recognized text off ${pagesInWords(pages, pageCount)}; the pictures stay. Undo with Ctrl Z.`;
}

/** the banner on a document with scanned pages. */
export function bannerSentence(unread: number, pageCount: number): string {
  if (unread === pageCount) return pageCount === 1 ? "This page is a scan, so its words can’t be searched or copied yet." : "This document is scanned, so its words can’t be searched or copied yet.";
  return `${plural(unread, "page")} of this document ${unread === 1 ? "is a scan" : "are scans"}, so ${unread === 1 ? "its" : "their"} words can’t be searched or copied yet.`;
}

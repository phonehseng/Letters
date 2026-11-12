// the "what will happen" line under each tool's button: one plain
// sentence that says what the tool is about to do to which pages, and
// whether this document or a new file changes. it updates as the
// settings change, so nobody has to guess before clicking.
//
// pages are counted from 0 here and written from 1, as people count.

import type {
  HeaderFooterSettings,
  NumberFormat,
  PageNumberSettings,
  SplitSettings,
  StampKind,
  StampLayer,
} from "./pageApi";
import { describePages, pageCountText } from "./pageRanges";

/** the closing words of every change to this document. */
export const UNDO_HINT = "Undo with Ctrl Z.";
const NEW_FILES_ONLY = "This document doesn’t change.";

/**
 * which pages a tool will act on, said plainly: the selection, or the page
 * being read with a pointer to how to choose more.
 */
export function targetHint(target: { pages: number[]; fromSelection: boolean }, pageCount: number): string {
  const pages = describePages(target.pages, pageCount);
  if (target.fromSelection) return `Works on the selected pages: ${pages}.`;
  return `Works on the page you are reading, ${pages}. To choose other pages, select them in the sidebar.`;
}

/** "Page 3" style words for where pages go in: before 1, after n, at the end. */
export function insertPlace(atIndex: number, pageCount: number): string {
  if (atIndex <= 0) return "before page 1";
  if (atIndex >= pageCount) return "at the end";
  return `after page ${atIndex}`;
}

const capitalise = (text: string) => text.charAt(0).toUpperCase() + text.slice(1);

export function rotateEffect(quarterTurns: number, pages: number[], pageCount: number): string {
  const turn = quarterTurns === 2 ? "upside down" : `a quarter turn to the ${quarterTurns < 0 ? "left" : "right"}`;
  return `Turns ${describePages(pages, pageCount)} ${turn}. ${UNDO_HINT}`;
}

/** the sentence, or why the pages can't be deleted. */
export function deleteEffect(pages: number[], pageCount: number): { text: string; blocked: boolean } {
  if (pages.length >= pageCount) {
    return { text: "A PDF needs at least one page, so not every page can be deleted.", blocked: true };
  }
  return { text: `Takes ${describePages(pages, pageCount)} out of this document. ${UNDO_HINT}`, blocked: false };
}

export function insertBlankEffect(count: number, atIndex: number, pageCount: number): string {
  const what = count === 1 ? "1 blank page" : `${count} blank pages`;
  return `Adds ${what} ${insertPlace(atIndex, pageCount)}. ${UNDO_HINT}`;
}

/** before a file is chosen there is nothing to describe but the next step. */
export function insertFromEffect(
  file: { name: string; pageCount: number } | null,
  chosenCount: number | null,
  atIndex: number,
  pageCount: number,
): string {
  if (!file) return "Choose a PDF to copy pages from.";
  const what =
    chosenCount === null || chosenCount === file.pageCount
      ? `all ${pageCountText(file.pageCount)} of “${file.name}”`
      : `${pageCountText(chosenCount)} of “${file.name}”`;
  return `Copies ${what} in ${insertPlace(atIndex, pageCount)}. “${file.name}” doesn’t change. ${UNDO_HINT}`;
}

export function extractEffect(pages: number[], pageCount: number): string {
  return `Saves a copy of ${describePages(pages, pageCount)} as a new PDF. ${NEW_FILES_ONLY}`;
}

/** how many files a split by ranges or by a page count makes, when that can be known. */
function splitPartCount(settings: SplitSettings, pageCount: number): number | null {
  if (settings.mode === "byRanges") return settings.ranges.length;
  if (settings.mode === "everyNPages") return Math.ceil(pageCount / Math.max(1, settings.pagesPerPart));
  return null;
}

export function splitEffect(settings: SplitSettings, pageCount: number): string {
  const count = splitPartCount(settings, pageCount) ?? 0;
  const files = count === 1 ? "1 new PDF" : `${count} new PDFs`;
  let what: string;
  if (settings.mode === "byRanges") what = `${files}, one for each range,`;
  else if (settings.mode === "everyNPages") what = `${files} of up to ${pageCountText(settings.pagesPerPart)} each`;
  else if (settings.mode === "byBookmarks") what = "a new PDF for each chapter in the outline,";
  else what = `new PDFs of up to ${megabytes(settings.maxBytes)} each`;
  return `Makes ${what} in the folder you choose. Files already there are never replaced. ${NEW_FILES_ONLY}`;
}

export function megabytes(bytes: number): string {
  const value = bytes / (1024 * 1024);
  return `${Number.isInteger(value) ? value : value.toFixed(1)} MB`;
}

export function mergeEffect(fileCount: number): string {
  if (fileCount < 2) return "Add at least two PDFs to merge.";
  return `Makes one new PDF from ${fileCount} PDFs, in the order listed, with a bookmark for each. The PDFs themselves don’t change.`;
}

export function cropEffect(pages: number[], pageCount: number): string {
  return `Hides everything outside the rectangle on ${describePages(pages, pageCount)}. The hidden part stays in the file. ${UNDO_HINT}`;
}

/** "at the bottom centre", "at the top left". */
export function placeWords(vertical: "top" | "bottom", horizontal: "left" | "center" | "right"): string {
  const across = { left: "left", center: "centre", right: "right" }[horizontal];
  return `at the ${vertical} ${across}`;
}

/** the label a page number gets, as core/pages writes it. */
export function pageNumberLabel(shown: number, last: number, format: NumberFormat): string {
  if (format === "pageWord") return `Page ${shown}`;
  if (format === "ofTotal") return `${shown} of ${last}`;
  if (format === "roman") return roman(shown);
  return String(shown);
}

function roman(value: number): string {
  if (value <= 0 || value > 3999) return String(value);
  const steps: [number, string][] = [
    [1000, "m"], [900, "cm"], [500, "d"], [400, "cd"], [100, "c"], [90, "xc"],
    [50, "l"], [40, "xl"], [10, "x"], [9, "ix"], [5, "v"], [4, "iv"], [1, "i"],
  ];
  let left = value;
  let text = "";
  for (const [step, letters] of steps) {
    while (left >= step) {
      text += letters;
      left -= step;
    }
  }
  return text;
}

/** e.g. Adds “Page 1 of 30” at the bottom centre of 30 pages. Undo with Ctrl Z. */
export function pageNumbersEffect(settings: PageNumberSettings, pageCount: number): string {
  const chosen = settings.pages ?? Array.from({ length: pageCount }, (_, page) => page);
  const numbered = settings.skipFirstPage ? chosen.length - 1 : chosen.length;
  if (numbered <= 0) return "Choose more than one page, or don’t skip the first page.";
  const last = settings.startNumber + chosen.length - 1;
  const first = settings.startNumber + (settings.skipFirstPage ? 1 : 0);
  const sample = pageNumberLabel(first, last, settings.format);
  const where = placeWords(settings.position.vertical, settings.position.horizontal);
  const skip = settings.skipFirstPage ? ", leaving the first page without a number" : "";
  return `Adds “${sample}” ${where} of ${pageCountText(numbered)}${skip}. ${UNDO_HINT}`;
}

/** fills in the tokens the way core/pages does, for the sample in the sentence. */
export function fillTokens(template: string, page: number, pages: number, date: string, file: string): string {
  return template.replace(/\{(pages|page|date|file)\}/g, (_, token: string) =>
    token === "pages" ? String(pages) : token === "page" ? String(page) : token === "date" ? date : file,
  );
}

export function headerFooterEffect(settings: HeaderFooterSettings, pageCount: number): string {
  const texts = [settings.left, settings.center, settings.right].filter((text) => text.trim() !== "");
  if (texts.length === 0) return `Type some text for the ${settings.edge === "top" ? "header" : "footer"} first.`;
  const chosen = settings.pages ?? Array.from({ length: pageCount }, (_, page) => page);
  const firstPage = chosen.length > 0 ? Math.min(...chosen) + 1 : 1;
  const sample = texts.map((text) => fillTokens(text, firstPage, pageCount, settings.date, settings.fileName)).join("  ");
  const what = settings.edge === "top" ? "a header" : "a footer";
  return `Adds ${what} to ${pageCountText(chosen.length)}, starting “${sample}”. ${UNDO_HINT}`;
}

export function watermarkEffect(
  what: { kind: "text"; text: string } | { kind: "picture"; name: string | null },
  layer: StampLayer,
  opacity: number,
  pages: number[] | null,
  pageCount: number,
): string {
  if (what.kind === "text" && what.text.trim() === "") return "Type the watermark text first.";
  if (what.kind === "picture" && !what.name) return "Choose a PNG or JPEG picture first.";
  const subject = what.kind === "text" ? `“${what.text.trim()}”` : `the picture “${what.name}”`;
  const count = pages?.length ?? pageCount;
  const side = layer === "behind" ? "behind the page’s own text and pictures" : "in front of the page";
  return `Adds ${subject} across the middle of ${pageCountText(count)}, ${side}, at ${Math.round(opacity * 100)}% opacity. ${UNDO_HINT}`;
}

const STAMP_WORDS: Record<StampKind, string> = {
  pageNumbers: "page numbers",
  headerFooter: "headers and footers",
  watermark: "watermarks",
};

export function removeEffect(kind: StampKind, pages: number[] | null, pageCount: number): string {
  const which = pages === null ? `all ${pageCountText(pageCount)}` : describePages(pages, pageCount);
  return `${capitalise(`takes the ${STAMP_WORDS[kind]} that Letters added off ${which}`)}. Nothing else on the pages changes. ${UNDO_HINT}`;
}

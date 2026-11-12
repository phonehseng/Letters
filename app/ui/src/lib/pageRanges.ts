// typing pages as text, like "1-3, 7, 10-", and writing pages back as
// short text. people count pages from 1; the engine counts from 0, so
// everything returned here is 0-based.

/** a run of pages from `start` to `end`, both included, counted from 0. */
export interface PageSpan {
  start: number;
  end: number;
}

export type RangeResult =
  | {
      ok: true;
      /** every page named, once each, in order */
      pages: number[];
      /** the runs as typed, in the order typed (they may overlap) */
      spans: PageSpan[];
    }
  | { ok: false; error: string };

const EXAMPLE = "such as 1-3, 7";

/**
 * reads page text. parts are separated by commas or spaces; a part is a
 * page ("7"), a run ("1-3"), a run to the end ("10-") or from the start ("-4").
 */
export function parsePageRanges(text: string, pageCount: number): RangeResult {
  // an en dash is what many keyboards and autocorrect produce
  const parts = text
    .replace(/[–—]/g, "-")
    .replace(/\s*-\s*/g, "-")
    .split(/[\s,;]+/)
    .filter((part) => part.length > 0);
  if (parts.length === 0) return { ok: false, error: `Type the pages, ${EXAMPLE}.` };
  const spans: PageSpan[] = [];
  for (const part of parts) {
    const span = readPart(part, pageCount);
    if ("error" in span) return { ok: false, error: span.error };
    spans.push(span);
  }
  return { ok: true, pages: pagesIn(spans), spans };
}

function readPart(part: string, pageCount: number): PageSpan | { error: string } {
  const match = /^(\d*)-(\d*)$/.exec(part);
  if (!match && !/^\d+$/.test(part)) {
    return { error: `“${part}” isn’t a page number. Use numbers, commas and dashes, ${EXAMPLE}.` };
  }
  const first = match ? match[1] : part;
  const last = match ? match[2] : part;
  if (match && first === "" && last === "") return { error: `Put a page number before or after the dash, ${EXAMPLE}.` };
  const start = first === "" ? 1 : Number(first);
  const end = last === "" ? pageCount : Number(last);
  for (const page of [start, end]) {
    if (page < 1) return { error: "Pages are counted from 1." };
    if (page > pageCount) return { error: `Page ${page} doesn’t exist. This document has ${pageCountText(pageCount)}.` };
  }
  if (start > end) return { error: `“${part}” goes backwards. Write the lower page first, such as ${end}-${start}.` };
  return { start: start - 1, end: end - 1 };
}

function pagesIn(spans: PageSpan[]): number[] {
  const set = new Set<number>();
  for (const span of spans) for (let page = span.start; page <= span.end; page += 1) set.add(page);
  return [...set].sort((a, b) => a - b);
}

/** "1 page" or "30 pages". */
export function pageCountText(count: number): string {
  return `${count} ${count === 1 ? "page" : "pages"}`;
}

/** 0-based pages as short text people read: "1–3, 7". */
export function formatPages(pages: number[]): string {
  const sorted = [...new Set(pages)].sort((a, b) => a - b);
  const runs: string[] = [];
  let index = 0;
  while (index < sorted.length) {
    let end = index;
    while (end + 1 < sorted.length && sorted[end + 1] === sorted[end] + 1) end += 1;
    const first = sorted[index] + 1;
    const last = sorted[end] + 1;
    runs.push(first === last ? `${first}` : `${first}–${last}`);
    index = end + 1;
  }
  return runs.join(", ");
}

/** the same pages as text that can be typed back in: "1-3, 7". */
export function pagesAsTypedText(pages: number[]): string {
  return formatPages(pages).replace(/–/g, "-");
}

/**
 * the pages as words in a sentence: "page 3", "pages 1–3 and 7",
 * "all 30 pages".
 */
export function describePages(pages: number[], pageCount: number): string {
  const unique = [...new Set(pages)];
  if (unique.length === pageCount && pageCount > 1) return `all ${pageCount} pages`;
  if (unique.length === 1) return `page ${unique[0] + 1}`;
  const text = formatPages(unique);
  const lastComma = text.lastIndexOf(", ");
  const joined = lastComma === -1 ? text : `${text.slice(0, lastComma)} and ${text.slice(lastComma + 2)}`;
  return `pages ${joined}`;
}

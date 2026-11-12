// the areas marked for redaction but not yet applied.
//
// redaction is two steps, as in professional tools: mark first, apply
// later. a mark lives only here, in the window, never in the file: it is
// drawn as an outlined, hatched box so it can't be mistaken for a real
// redaction, it can be moved, resized and removed, and nothing is taken
// out of the document until Apply. these are plain functions on a list,
// so all of it is unit tested.

import type { Rect } from "../pageTools/pagePoints";
import { describePages } from "../pageRanges";
import type { RedactionArea } from "./protectApi";

/** how a mark was made. */
export type MarkSource = "text" | "area" | "found";

export interface PendingMark {
  id: string;
  /** counted from 0 */
  page: number;
  /** points from the page's visible top left, y down */
  rect: Rect;
  source: MarkSource;
  /** the text it was made from, when known (shown in the list) */
  label: string;
}

export type NewMark = Omit<PendingMark, "id">;

/** two boxes closer than this, in points, on every side are the same mark. */
const SAME_PLACE = 0.5;

function samePlace(a: Rect, b: Rect): boolean {
  return (
    Math.abs(a.x - b.x) < SAME_PLACE &&
    Math.abs(a.y - b.y) < SAME_PLACE &&
    Math.abs(a.width - b.width) < SAME_PLACE &&
    Math.abs(a.height - b.height) < SAME_PLACE
  );
}

/** makes ids that are unique in this window. */
export function markIds(prefix = "mark"): () => string {
  let next = 1;
  return () => `${prefix}-${next++}`;
}

/**
 * adds marks, skipping any already marked in the same place (finding the
 * same phrase twice must not mark it twice). returns the new list and the
 * marks that were really added.
 */
export function addMarks(marks: PendingMark[], added: NewMark[], newId: () => string): { marks: PendingMark[]; added: PendingMark[] } {
  const made: PendingMark[] = [];
  for (const mark of added) {
    if (mark.rect.width <= 0 || mark.rect.height <= 0) continue;
    const taken = [...marks, ...made].some((other) => other.page === mark.page && samePlace(other.rect, mark.rect));
    if (!taken) made.push({ ...mark, id: newId() });
  }
  return { marks: [...marks, ...made], added: made };
}

export function removeMark(marks: PendingMark[], id: string): PendingMark[] {
  return marks.filter((mark) => mark.id !== id);
}

export function moveMark(marks: PendingMark[], id: string, rect: Rect): PendingMark[] {
  return marks.map((mark) => (mark.id === id ? { ...mark, rect } : mark));
}

export function marksOn(marks: PendingMark[], page: number): PendingMark[] {
  return marks.filter((mark) => mark.page === page);
}

/** the marks in reading order: by page, then top to bottom, then left to right. */
export function orderedMarks(marks: PendingMark[]): PendingMark[] {
  return [...marks].sort((a, b) => a.page - b.page || a.rect.y - b.rect.y || a.rect.x - b.rect.x);
}

/**
 * what the engine is asked to redact. the boxes come in the order of
 * `orderedMarks`, which is also the order of the engine's preview areas,
 * so preview area n is ordered mark n.
 */
export function asRedactionAreas(marks: PendingMark[]): RedactionArea[] {
  const areas: RedactionArea[] = [];
  for (const mark of orderedMarks(marks)) {
    const last = areas[areas.length - 1];
    if (last && last.page === mark.page) last.rects.push(mark.rect);
    else areas.push({ page: mark.page, rects: [mark.rect] });
  }
  return areas;
}

export function markedPages(marks: PendingMark[]): number[] {
  return [...new Set(marks.map((mark) => mark.page))].sort((a, b) => a - b);
}

/** "1 area" or "3 areas". */
export function areaCount(count: number): string {
  return count === 1 ? "1 area" : `${count} areas`;
}

/** where the marks are, for the list and screen readers: "3 areas marked on page 2." */
export function markedSentence(marks: PendingMark[], pageCount: number): string {
  if (marks.length === 0) return "No areas are marked.";
  return `${areaCount(marks.length)} marked on ${describePages(markedPages(marks), pageCount)}.`;
}

/**
 * what one page looks like, to tell whether a mark still belongs where
 * it was made: its size and the start of its text. another tool can
 * move, turn, crop or delete pages; a page whose look changed loses its
 * marks rather than have them land on the wrong words.
 */
export function pageSignature(size: { width: number; height: number } | undefined, text: string): string {
  if (!size) return "gone";
  return `${Math.round(size.width)}x${Math.round(size.height)}:${text.slice(0, 80)}`;
}

/**
 * keeps the marks whose page still looks the way it did when they were
 * made. `before` and `now` map pages to signatures. returns the kept
 * marks and the pages whose marks were dropped.
 */
export function keepMatchingPages(
  marks: PendingMark[],
  before: Map<number, string>,
  now: Map<number, string>,
): { kept: PendingMark[]; droppedPages: number[] } {
  const dropped = new Set<number>();
  for (const page of markedPages(marks)) {
    const then = before.get(page);
    if (then !== undefined && then !== now.get(page)) dropped.add(page);
  }
  return {
    kept: marks.filter((mark) => !dropped.has(mark.page)),
    droppedPages: [...dropped].sort((a, b) => a - b),
  };
}

/** a mark's line in the list: its text, or what kind of area it is. */
export function markLabel(mark: PendingMark): string {
  if (mark.label.trim()) return mark.label.trim();
  return mark.source === "area" ? "Area you drew" : "Marked area";
}

/** a little room around selected text, in points, so the edges of letters are covered. */
export const TEXT_MARGIN = 1;

/** a line box from the text selection, grown by the margin and kept on the page. */
export function padTextBox(rect: Rect, page: { width: number; height: number }): Rect {
  const x = Math.max(0, rect.x - TEXT_MARGIN);
  const y = Math.max(0, rect.y - TEXT_MARGIN);
  const right = Math.min(page.width, rect.x + rect.width + TEXT_MARGIN);
  const bottom = Math.min(page.height, rect.y + rect.height + TEXT_MARGIN);
  return { x, y, width: right - x, height: bottom - y };
}

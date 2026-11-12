// Find and Mark: looking for a phrase or a kind of text, and showing
// each match with a little of the text around it, so the person can tell
// which ones to mark.

import type { PageRect } from "../api";
import type { PageText } from "../pageTools/textSelection";
import type { NewMark } from "./pendingMarks";
import type { FoundText, TextQuery } from "./protectApi";

export type PatternKind = "emailAddresses" | "phoneNumbers" | "longNumbers";

export const PATTERNS: { kind: PatternKind; label: string; description: string }[] = [
  { kind: "emailAddresses", label: "Email Addresses", description: "such as name@school.edu" },
  { kind: "phoneNumbers", label: "Phone Numbers", description: "with 7 to 15 digits" },
  { kind: "longNumbers", label: "Long Numbers", description: "8 digits or more, such as ID or card numbers" },
];

/** the engine's query for what the person typed or picked. */
export function queryFor(search: { phrase: string; matchCase: boolean } | { pattern: PatternKind }): TextQuery {
  if ("pattern" in search) return { kind: search.pattern };
  return { kind: "phrase", text: search.phrase, matchCase: search.matchCase };
}

/** a match with its place in the list. */
export interface FoundItem extends FoundText {
  key: string;
  chosen: boolean;
}

export function asItems(found: FoundText[]): FoundItem[] {
  return found.map((match, index) => ({ ...match, key: `found-${index}`, chosen: true }));
}

function centreInside(rect: PageRect, boxes: PageRect[]): boolean {
  const x = rect.x + rect.width / 2;
  const y = rect.y + rect.height / 2;
  return boxes.some((box) => x >= box.x && x <= box.x + box.width && y >= box.y && y <= box.y + box.height);
}

/**
 * the words around a match, on its own lines only (the line before may
 * hold something else private): the characters whose middle is inside
 * the match's boxes are the match, and up to `radius` characters of the
 * same lines on each side are the context. empty when the page's text
 * isn't known.
 */
export function contextAround(text: PageText | undefined, rects: PageRect[], radius = 30): { before: string; after: string } {
  if (!text) return { before: "", after: "" };
  const inside = text.chars.flatMap((char, index) => (centreInside(char.rect, rects) ? [index] : []));
  if (inside.length === 0) return { before: "", after: "" };
  const first = Math.min(...inside);
  const last = Math.max(...inside);
  let start = first;
  while (start > 0 && first - start < radius && text.chars[start - 1].line === text.chars[first].line) start -= 1;
  let end = last + 1;
  while (end < text.chars.length && end - last - 1 < radius && text.chars[end].line === text.chars[last].line) end += 1;
  const piece = (from: number, to: number) => text.chars.slice(from, to).map((char) => char.ch).join("").replace(/\s+/g, " ");
  const cutBefore = start > 0 && text.chars[start - 1].line === text.chars[first].line;
  const cutAfter = end < text.chars.length && text.chars[end].line === text.chars[last].line;
  const before = piece(start, first).trimStart();
  const after = piece(last + 1, end).trimEnd();
  return { before: cutBefore ? `…${before}` : before, after: cutAfter ? `${after}…` : after };
}

/** the chosen matches as marks, one per line of each match. */
export function marksFromFound(items: FoundItem[]): NewMark[] {
  return items.filter((item) => item.chosen).flatMap((item) => item.rects.map((rect) => ({ page: item.page, rect, source: "found" as const, label: item.text })));
}

/** the line above the list: "4 matches on pages 1 and 3." */
export function foundSentence(items: FoundItem[], describe: (pages: number[]) => string, capped: boolean): string {
  if (items.length === 0) return "No matches.";
  const count = items.length === 1 ? "1 match" : `${items.length} matches`;
  const pages = describe([...new Set(items.map((item) => item.page))]);
  return capped ? `The first ${count}, on ${pages}. Narrow the search to see the rest.` : `${count} on ${pages}.`;
}

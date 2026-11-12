// the outline panel: the document's headings, live, with promote,
// demote and moving a whole section (a heading and everything under it)
// by dragging. pure functions on the editor document, so they are tested
// without a screen.

import type { Node as PmNode } from "prosemirror-model";
import type { Transaction } from "prosemirror-state";

export interface OutlineEntry {
  /** index of the heading among the document's top-level blocks */
  index: number;
  /** where the heading starts in the document */
  pos: number;
  level: number;
  text: string;
}

/** every top-level heading, in order. */
export function buildOutline(doc: PmNode): OutlineEntry[] {
  const entries: OutlineEntry[] = [];
  doc.forEach((node, offset, index) => {
    if (node.type.name !== "heading") return;
    entries.push({ index, pos: offset, level: node.attrs.level as number, text: node.textContent.trim() });
  });
  return entries;
}

/** the outline entry the cursor is in or under, for marking it in the panel. */
export function currentEntry(entries: OutlineEntry[], pos: number): OutlineEntry | null {
  let found: OutlineEntry | null = null;
  for (const entry of entries) {
    if (entry.pos > pos) break;
    found = entry;
  }
  return found;
}

/**
 * the top-level blocks a heading's section covers: the heading and every
 * block after it up to the next heading of the same or a higher level.
 */
export function sectionBlocks(doc: PmNode, headingIndex: number): { first: number; end: number } {
  const heading = doc.child(headingIndex);
  const level = heading.attrs.level as number;
  let end = headingIndex + 1;
  while (end < doc.childCount) {
    const next = doc.child(end);
    if (next.type.name === "heading" && (next.attrs.level as number) <= level) break;
    end += 1;
  }
  return { first: headingIndex, end };
}

/** the document position before top-level block `index` (or the end). */
export function positionOfBlock(doc: PmNode, index: number): number {
  let pos = 0;
  for (let i = 0; i < Math.min(index, doc.childCount); i += 1) pos += doc.child(i).nodeSize;
  return pos;
}

/**
 * moves the section of the heading at `headingIndex` so it starts before
 * top-level block `beforeIndex`. a drop inside the section itself changes
 * nothing. returns false when nothing moved.
 */
export function moveSection(tr: Transaction, headingIndex: number, beforeIndex: number): boolean {
  const doc = tr.doc;
  const { first, end } = sectionBlocks(doc, headingIndex);
  if (beforeIndex >= first && beforeIndex <= end) return false;
  const from = positionOfBlock(doc, first);
  const to = positionOfBlock(doc, end);
  const slice = doc.slice(from, to);
  const target = positionOfBlock(doc, beforeIndex);
  // insert first, then delete the original (its position shifts when the
  // target is before it)
  tr.insert(target, slice.content);
  const shift = target <= from ? to - from : 0;
  tr.delete(from + shift, to + shift);
  return true;
}

/** a heading one level up (2 becomes 1) or down; null at the ends. */
export function shiftedLevel(level: number, direction: "promote" | "demote"): number | null {
  const next = direction === "promote" ? level - 1 : level + 1;
  return next >= 1 && next <= 6 ? next : null;
}

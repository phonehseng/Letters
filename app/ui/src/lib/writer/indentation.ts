// "smart indentation": what Tab and Shift Tab do at the start of a
// paragraph, the way a word processor does it.
//
// - Tab at the start of a plain paragraph gives it a half-inch first-line
//   indent; a second Tab moves the whole paragraph in by half an inch.
// - Shift Tab takes those back one step at a time.
// - In a list, Tab and Shift Tab change the item's level instead.
// - Backspace at the start of a paragraph with a first-line indent removes
//   the indent before it joins the paragraph to the one above.

import type { Indent, ListRef } from "./model";

export const INDENT_STEP = 720;
export const MAX_LIST_LEVEL = 8;

/** the indent after Tab at the start of a paragraph (offset is first line minus left). */
export function indentAfterTab(indent: Indent): Indent {
  const left = indent.left ?? 0;
  if (indent.hanging) return { ...indent, left: left + INDENT_STEP };
  if (!indent.firstLine) return { ...indent, firstLine: INDENT_STEP, hanging: undefined };
  return { ...indent, left: left + INDENT_STEP };
}

/** the indent after Shift Tab, or null when there is nothing to take back. */
export function indentAfterShiftTab(indent: Indent): Indent | null {
  const left = indent.left ?? 0;
  if (left >= INDENT_STEP) return { ...indent, left: left - INDENT_STEP };
  if (indent.firstLine && indent.firstLine > 0) return { ...indent, firstLine: 0 };
  if (left > 0) return { ...indent, left: 0 };
  return null;
}

/** a list item one level deeper or shallower, or null at the ends. */
export function listLevelAfter(list: ListRef, direction: "in" | "out"): ListRef | null {
  const level = direction === "in" ? list.level + 1 : list.level - 1;
  if (level < 0 || level > MAX_LIST_LEVEL) return null;
  return { ...list, level };
}

/** Backspace at the start: drop a first-line indent before anything else. */
export function indentAfterBackspace(indent: Indent): Indent | null {
  if (indent.firstLine && indent.firstLine > 0) return { ...indent, firstLine: 0 };
  return null;
}

/** an indent with nothing in it, so a paragraph does not store an empty one. */
export function cleanIndent(indent: Indent): Indent | undefined {
  const clean = Object.fromEntries(Object.entries(indent).filter(([, value]) => value !== undefined)) as Indent;
  return Object.keys(clean).length ? clean : undefined;
}

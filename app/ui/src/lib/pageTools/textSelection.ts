// selecting text on pages, the way a word processor does it.
//
// the engine says where every character of a page is (page_text). a
// selection is two carets: the anchor, where it started, and the focus,
// the end that moves. a caret sits between characters: offset 0 is
// before the first character of the page, offset n after the last.
// a selection can run across lines and across pages.
//
// double-clicking selects a word and triple-clicking a line; dragging
// after either keeps whole words or lines. shift with the arrows moves
// the focus from the keyboard.
//
// nothing here touches the screen, so all of it is unit tested.

import type { Point, Rect } from "./pagePoints";

/** one character of a page, as the engine reports it. */
export interface TextChar {
  ch: string;
  rect: Rect;
  /** the line it is on, counted from 0 in reading order */
  line: number;
}

export interface PageText {
  chars: TextChar[];
  /** one box per line */
  lines: Rect[];
}

/** the text of the pages loaded so far, by page number. */
export type TextLookup = (page: number) => PageText | undefined;

export interface Caret {
  page: number;
  /** 0 is before the page's first character */
  offset: number;
}

export type Granularity = "character" | "word" | "line";

export interface TextSelection {
  anchor: Caret;
  focus: Caret;
  /** what a drag extends by: characters, whole words or whole lines */
  granularity: Granularity;
  /** the word or line first clicked, kept whole while dragging */
  origin: { start: Caret; end: Caret };
}

/** below zero when `a` comes first in reading order. */
export function compareCarets(a: Caret, b: Caret): number {
  return a.page !== b.page ? a.page - b.page : a.offset - b.offset;
}

/** the selection's two ends in reading order. */
export function orderedEnds(selection: TextSelection): { start: Caret; end: Caret } {
  const { anchor, focus } = selection;
  return compareCarets(anchor, focus) <= 0 ? { start: anchor, end: focus } : { start: focus, end: anchor };
}

export function isCollapsed(selection: TextSelection | null): boolean {
  return !selection || compareCarets(selection.anchor, selection.focus) === 0;
}

/** the first and one-past-last character of a line. lines are runs of the list. */
export function lineSpan(text: PageText, line: number): { start: number; end: number } {
  let start = text.chars.findIndex((char) => char.line === line);
  if (start < 0) return { start: 0, end: 0 };
  let end = start;
  while (end < text.chars.length && text.chars[end].line === line) end += 1;
  // characters of one line are always next to each other
  while (start > 0 && text.chars[start - 1].line === line) start -= 1;
  return { start, end };
}

/** whether a line runs down the screen (a turned page) rather than across. */
function runsDown(text: PageText, span: { start: number; end: number }): boolean {
  if (span.end - span.start < 2) {
    const box = text.lines[text.chars[span.start]?.line ?? 0];
    return box ? box.height > box.width * 2 : false;
  }
  const first = text.chars[span.start].rect;
  const last = text.chars[span.end - 1].rect;
  return Math.abs(last.y - first.y) > Math.abs(last.x - first.x);
}

function distanceToRect(rect: Rect, point: Point): number {
  const dx = Math.max(rect.x - point.x, 0, point.x - (rect.x + rect.width));
  const dy = Math.max(rect.y - point.y, 0, point.y - (rect.y + rect.height));
  return Math.hypot(dx, dy);
}

/** the line nearest a point, or -1 for a page without text. */
export function nearestLine(text: PageText, point: Point): number {
  let best = -1;
  let bestDistance = Infinity;
  text.lines.forEach((box, line) => {
    const distance = distanceToRect(box, point);
    if (distance < bestDistance) {
      best = line;
      bestDistance = distance;
    }
  });
  return best;
}

/**
 * the caret nearest a point: on the nearest line, before or after the
 * character under the point depending on which half of it was hit.
 */
export function caretAt(text: PageText, page: number, point: Point): Caret {
  const line = nearestLine(text, point);
  if (line < 0) return { page, offset: 0 };
  const span = lineSpan(text, line);
  const down = runsDown(text, span);
  const along = (rect: Rect) => (down ? { start: rect.y, size: rect.height } : { start: rect.x, size: rect.width });
  const position = down ? point.y : point.x;
  for (let index = span.start; index < span.end; index += 1) {
    const { start, size } = along(text.chars[index].rect);
    if (position < start + size / 2) return { page, offset: index };
  }
  return { page, offset: span.end };
}

/** how far a point is from the nearest text on the page, in points. */
export function distanceToText(text: PageText, point: Point): number {
  const line = nearestLine(text, point);
  return line < 0 ? Infinity : distanceToRect(text.lines[line], point);
}

type CharClass = "word" | "space" | "other";

function classOf(ch: string): CharClass {
  if (/\s/u.test(ch)) return "space";
  // letters, digits and the joiners inside words such as "don't" and "x-ray"
  if (/[\p{L}\p{N}_'’-]/u.test(ch)) return "word";
  return "other";
}

/**
 * the word around a caret: a run of letters and digits, a run of spaces,
 * or a single other character. it never crosses a line.
 */
export function wordSpan(text: PageText, offset: number): { start: number; end: number } {
  if (text.chars.length === 0) return { start: 0, end: 0 };
  // a caret after the last character means the word before it
  const index = Math.min(offset, text.chars.length - 1);
  const line = text.chars[index].line;
  const kind = classOf(text.chars[index].ch);
  const same = (at: number) => text.chars[at].line === line && classOf(text.chars[at].ch) === kind;
  if (kind === "other") return { start: index, end: index + 1 };
  let start = index;
  let end = index + 1;
  while (start > 0 && same(start - 1)) start -= 1;
  while (end < text.chars.length && same(end)) end += 1;
  return { start, end };
}

/** the whole line around a caret. */
export function lineSpanAt(text: PageText, offset: number): { start: number; end: number } {
  if (text.chars.length === 0) return { start: 0, end: 0 };
  const index = Math.min(offset, text.chars.length - 1);
  return lineSpan(text, text.chars[index].line);
}

function spanFor(text: PageText, offset: number, granularity: Granularity) {
  if (granularity === "word") return wordSpan(text, offset);
  if (granularity === "line") return lineSpanAt(text, offset);
  return { start: offset, end: offset };
}

/** how many clicks mean what: one a caret, two a word, three a line. */
export function granularityFor(clicks: number): Granularity {
  if (clicks >= 3) return "line";
  if (clicks === 2) return "word";
  return "character";
}

/** a selection starting at a caret: empty, or the word or line clicked. */
export function startSelection(text: PageText, caret: Caret, granularity: Granularity): TextSelection {
  const span = spanFor(text, caret.offset, granularity);
  const start = { page: caret.page, offset: span.start };
  const end = { page: caret.page, offset: span.end };
  return { anchor: start, focus: end, granularity, origin: { start, end } };
}

/**
 * the selection after the pointer moved to `caret`. the word or line
 * first clicked always stays selected, and the moving end snaps to whole
 * words or lines when that is how the selection began.
 */
export function extendSelection(selection: TextSelection, text: PageText, caret: Caret): TextSelection {
  const span = spanFor(text, caret.offset, selection.granularity);
  const { origin } = selection;
  if (compareCarets(caret, origin.start) < 0) {
    return { ...selection, anchor: origin.end, focus: { page: caret.page, offset: span.start } };
  }
  const end = { page: caret.page, offset: Math.max(span.end, caret.offset) };
  if (compareCarets(end, origin.end) < 0) {
    return { ...selection, anchor: origin.start, focus: origin.end };
  }
  return { ...selection, anchor: origin.start, focus: end };
}

/** the character offsets selected on one page, or null when none are. */
export function selectedSpanOn(
  selection: TextSelection,
  page: number,
  text: PageText,
): { start: number; end: number } | null {
  const { start, end } = orderedEnds(selection);
  if (page < start.page || page > end.page) return null;
  const from = page === start.page ? start.offset : 0;
  const to = page === end.page ? end.offset : text.chars.length;
  return to > from ? { start: from, end: to } : null;
}

/**
 * the boxes that show the selection on one page: one per line, covering
 * the selected characters of that line. text markup uses the same boxes.
 */
export function selectionRects(selection: TextSelection, page: number, text: PageText): Rect[] {
  const span = selectedSpanOn(selection, page, text);
  if (!span) return [];
  const rects: Rect[] = [];
  let line = -1;
  for (let index = span.start; index < span.end; index += 1) {
    const char = text.chars[index];
    const box = char.rect;
    if (char.line !== line) {
      line = char.line;
      rects.push({ ...box });
      continue;
    }
    const last = rects[rects.length - 1];
    const right = Math.max(last.x + last.width, box.x + box.width);
    const bottom = Math.max(last.y + last.height, box.y + box.height);
    last.x = Math.min(last.x, box.x);
    last.y = Math.min(last.y, box.y);
    last.width = right - last.x;
    last.height = bottom - last.y;
  }
  // a line where only an invisible space was selected shows nothing
  return rects.filter((rect) => rect.width > 0.01 && rect.height > 0.01);
}

/** the pages a selection touches, in order. */
export function selectedPages(selection: TextSelection): number[] {
  const { start, end } = orderedEnds(selection);
  return Array.from({ length: end.page - start.page + 1 }, (_, step) => start.page + step);
}

/**
 * the selected words as text to copy: lines end with a line break, and
 * spaces at the ends of lines are dropped.
 */
export function selectedText(selection: TextSelection, texts: TextLookup): string {
  const lines: string[] = [];
  for (const page of selectedPages(selection)) {
    const text = texts(page);
    if (!text) continue;
    const span = selectedSpanOn(selection, page, text);
    if (!span) continue;
    let line = -1;
    for (let index = span.start; index < span.end; index += 1) {
      const char = text.chars[index];
      if (char.line !== line) {
        line = char.line;
        lines.push("");
      }
      lines[lines.length - 1] += char.ch;
    }
  }
  return lines.map((line) => line.trim()).join("\n");
}

export type CaretMove = "left" | "right" | "up" | "down" | "wordLeft" | "wordRight" | "lineStart" | "lineEnd";

/** the arrow keys, with or without ctrl (option on a mac), as caret moves. */
export function caretMoveForKey(key: string, byWord: boolean): CaretMove | null {
  if (key === "ArrowLeft") return byWord ? "wordLeft" : "left";
  if (key === "ArrowRight") return byWord ? "wordRight" : "right";
  if (key === "ArrowUp") return "up";
  if (key === "ArrowDown") return "down";
  if (key === "Home") return "lineStart";
  if (key === "End") return "lineEnd";
  return null;
}

/** the next page after `page` in `direction` that has text loaded, if any. */
function pageWithText(page: number, direction: 1 | -1, texts: TextLookup, pageCount: number): number | null {
  for (let next = page + direction; next >= 0 && next < pageCount; next += direction) {
    const text = texts(next);
    if (text === undefined) return null;
    if (text.chars.length > 0) return next;
  }
  return null;
}

/** the middle of a character along its line, for keeping the column when moving up and down. */
function caretPosition(text: PageText, offset: number): Point | null {
  const char = text.chars[Math.min(offset, text.chars.length - 1)];
  if (!char) return null;
  const atEnd = offset >= text.chars.length;
  const { rect } = char;
  return { x: atEnd ? rect.x + rect.width : rect.x, y: rect.y + rect.height / 2 };
}

/** a caret moved by one step from the keyboard. it may cross to another page. */
export function moveCaret(caret: Caret, move: CaretMove, texts: TextLookup, pageCount: number): Caret {
  const text = texts(caret.page);
  if (!text || text.chars.length === 0) return caret;
  const count = text.chars.length;
  const offset = Math.min(caret.offset, count);
  switch (move) {
    case "left": {
      if (offset > 0) return { page: caret.page, offset: offset - 1 };
      const previous = pageWithText(caret.page, -1, texts, pageCount);
      return previous === null ? caret : { page: previous, offset: texts(previous)!.chars.length };
    }
    case "right": {
      if (offset < count) return { page: caret.page, offset: offset + 1 };
      const next = pageWithText(caret.page, 1, texts, pageCount);
      return next === null ? caret : { page: next, offset: 0 };
    }
    case "wordLeft": {
      let at = offset;
      // skip the spaces before the caret, then the word before them
      while (at > 0 && classOf(text.chars[at - 1].ch) === "space") at -= 1;
      return { page: caret.page, offset: at > 0 ? wordSpan(text, at - 1).start : 0 };
    }
    case "wordRight": {
      let at = offset;
      while (at < count && classOf(text.chars[at].ch) === "space") at += 1;
      return { page: caret.page, offset: at < count ? wordSpan(text, at).end : count };
    }
    case "lineStart":
      return { page: caret.page, offset: lineSpanAt(text, offset).start };
    case "lineEnd":
      return { page: caret.page, offset: lineSpanAt(text, offset).end };
    case "up":
    case "down":
      return moveByLine(caret, move === "up" ? -1 : 1, text, texts, pageCount);
  }
}

function moveByLine(caret: Caret, direction: 1 | -1, text: PageText, texts: TextLookup, pageCount: number): Caret {
  const offset = Math.min(caret.offset, text.chars.length);
  const here = caretPosition(text, offset);
  const line = text.chars[Math.min(offset, text.chars.length - 1)].line;
  const target = line + direction;
  if (here && target >= 0 && target < text.lines.length) {
    const box = text.lines[target];
    return caretAt(text, caret.page, { x: here.x, y: box.y + box.height / 2 });
  }
  // past the first or last line: the first or last line of the next page
  const other = pageWithText(caret.page, direction, texts, pageCount);
  if (other === null) return { page: caret.page, offset: direction < 0 ? 0 : text.chars.length };
  const otherText = texts(other)!;
  const box = otherText.lines[direction < 0 ? otherText.lines.length - 1 : 0];
  return caretAt(otherText, other, { x: here?.x ?? box.x, y: box.y + box.height / 2 });
}

/** the selection with its focus moved by one step from the keyboard. */
export function extendByKey(selection: TextSelection, move: CaretMove, texts: TextLookup, pageCount: number): TextSelection {
  const focus = moveCaret(selection.focus, move, texts, pageCount);
  // from the keyboard a selection grows by characters, whatever started it
  return { anchor: selection.anchor, focus, granularity: "character", origin: { start: selection.anchor, end: selection.anchor } };
}

/** an empty selection at the start of the first line of a page, for starting from the keyboard. */
export function selectionAtPageStart(page: number): TextSelection {
  const caret = { page, offset: 0 };
  return { anchor: caret, focus: caret, granularity: "character", origin: { start: caret, end: caret } };
}

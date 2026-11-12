// text flowing around free objects on the editing surface.
//
// the approach: each object that pushes text away gets an invisible css
// float (an "exclusion") at the start of the paragraph whose top is just
// above the object. the float reaches from that paragraph's first line
// down to the object's bottom, and `shape-outside: inset(dy 0 0 0)` makes
// only its lower part (where the object is) push lines aside, so the
// lines above the object stay full width. browsers then wrap every
// following line around it, in that paragraph and the next ones, with
// their own line breaking.
//
// why floats and not measured spacers: the browser re-wraps lines itself
// as you type, with no measuring per keystroke, and an exclusion only
// changes when its object is dropped somewhere new. while an object is
// dragged only its picture moves (a css transform), so the text stays
// still and dragging stays smooth; it re-wraps once, on release.
//
// limits, said plainly: css floats push text to one side, so square,
// tight and through wrapping send the text to the wider side of the
// object (word can fill both sides); tight and through are drawn as
// square. two objects whose exclusions overlap vertically on the same
// side can sit next to each other instead of where they are.

import type { Rect, WrapMode } from "./objectGeometry";

/** word's default distance between a wrapped object and the text beside it: 0.125 in, in css pixels. */
export const WRAP_DISTANCE = 12;

export interface Exclusion {
  /** the top-level block whose start holds the float */
  block: number;
  side: "left" | "right";
  /** how far below that paragraph's top the object starts */
  dy: number;
  /** the float's width from its side of the column, the object and the gap included */
  width: number;
  /** from the paragraph's top to the object's bottom */
  height: number;
}

export interface WrapLayout {
  /** the text column's width in css pixels (the root is as wide as the column) */
  columnWidth: number;
  /** each top-level block's top in root coordinates, NaN when out of the flow */
  blockTops: number[];
  /** blocks that can hold a float: paragraphs and headings */
  holdsText: boolean[];
  /** for an object on page n: the bottom of that page's text area in root coordinates */
  pageTextBottom: (top: number) => number;
}

/** whether a wrap mode pushes text away at all. */
export function pushesText(wrap: WrapMode): boolean {
  return wrap !== "behindText" && wrap !== "inFrontOfText";
}

/** the exclusion that makes text flow around one object, or null when it pushes nothing aside. */
export function exclusionFor(rect: Rect, wrap: WrapMode, layout: WrapLayout): Exclusion | null {
  if (!pushesText(wrap)) return null;
  const host = hostBlock(rect.top, layout);
  if (host === null) return null;
  const hostTop = layout.blockTops[host];
  const dy = Math.max(0, rect.top - hostTop);
  const bottom = Math.min(rect.top + rect.height, layout.pageTextBottom(rect.top));
  const height = bottom - hostTop;
  if (height <= dy) return null;
  const column = layout.columnWidth;
  if (wrap === "topAndBottom") return { block: host, side: "left", dy, width: column, height };
  const left = rect.left - WRAP_DISTANCE;
  const right = rect.left + rect.width + WRAP_DISTANCE;
  if (right <= 0 || left >= column) return null;
  // text goes to the wider side, as word's "largest only" does
  const roomLeft = Math.max(0, left);
  const roomRight = Math.max(0, column - right);
  if (roomRight >= roomLeft) return { block: host, side: "left", dy, width: Math.min(column, Math.max(0, right)), height };
  return { block: host, side: "right", dy, width: Math.min(column, Math.max(0, column - left)), height };
}

/** the last paragraph in the flow whose top is at or above `top`; the first one when the object is above them all. */
export function hostBlock(top: number, layout: WrapLayout): number | null {
  let found: number | null = null;
  let first: number | null = null;
  layout.blockTops.forEach((blockTop, index) => {
    if (Number.isNaN(blockTop) || !layout.holdsText[index]) return;
    if (first === null) first = index;
    if (blockTop <= top + 0.5) found = index;
  });
  return found ?? first;
}

/** the float's inline style (the widget in objectLayer.ts uses it). */
export function exclusionStyle(exclusion: Exclusion): string {
  return [
    `float:${exclusion.side}`,
    `width:${exclusion.width.toFixed(2)}px`,
    `height:${exclusion.height.toFixed(2)}px`,
    `shape-outside:inset(${exclusion.dy.toFixed(2)}px 0 0 0)`,
  ].join(";");
}

/** two exclusion lists are the same on screen (to the hundredth of a pixel). */
export function sameExclusions(a: Exclusion[], b: Exclusion[]): boolean {
  return a.length === b.length && a.every((one, index) => exclusionStyle(one) === exclusionStyle(b[index]) && one.block === b[index].block);
}

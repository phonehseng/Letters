// the colours offered for comments and marks: a few fixed swatches per
// kind, named so a screen reader can say them.
//
// highlights are light, because the engine multiplies them onto the page
// and text under them must stay readable. ink, shapes and text are
// strong, so they show on paper and on a printout. the app's accent blue
// is not among them: it means "selected" on screen, and a mark in the
// same blue would look selected forever.

import type { Rgb } from "./annotateApi";

export interface Swatch {
  name: string;
  color: Rgb;
}

export const HIGHLIGHT_SWATCHES: Swatch[] = [
  { name: "Yellow", color: { r: 255, g: 222, b: 70 } },
  { name: "Green", color: { r: 140, g: 226, b: 120 } },
  { name: "Blue", color: { r: 150, g: 205, b: 255 } },
  { name: "Pink", color: { r: 255, g: 160, b: 205 } },
  { name: "Orange", color: { r: 255, g: 184, b: 96 } },
];

/** for underline, strike through, ink, shapes, lines and text. */
export const INK_SWATCHES: Swatch[] = [
  { name: "Black", color: { r: 28, g: 28, b: 30 } },
  { name: "Red", color: { r: 214, g: 40, b: 40 } },
  { name: "Blue", color: { r: 30, g: 80, b: 190 } },
  { name: "Green", color: { r: 30, g: 135, b: 60 } },
  { name: "Purple", color: { r: 120, g: 60, b: 170 } },
  { name: "Orange", color: { r: 230, g: 120, b: 20 } },
];

/** the background of a shape or text box. "None" is offered beside these. */
export const FILL_SWATCHES: Swatch[] = [
  { name: "White", color: { r: 255, g: 255, b: 255 } },
  { name: "Light Yellow", color: { r: 255, g: 246, b: 190 } },
  { name: "Light Blue", color: { r: 218, g: 236, b: 255 } },
  { name: "Light Grey", color: { r: 232, g: 232, b: 235 } },
];

export const NOTE_SWATCHES: Swatch[] = [
  { name: "Yellow", color: { r: 255, g: 204, b: 0 } },
  { name: "Green", color: { r: 90, g: 200, b: 90 } },
  { name: "Blue", color: { r: 90, g: 160, b: 240 } },
  { name: "Pink", color: { r: 245, g: 120, b: 170 } },
];

/** the highlighter pen for Draw: wide, yellow and see-through. */
export const HIGHLIGHTER = { color: HIGHLIGHT_SWATCHES[0].color, width: 14, opacity: 0.45 };

export function sameColor(a: Rgb | null | undefined, b: Rgb | null | undefined): boolean {
  if (!a || !b) return a === b;
  return a.r === b.r && a.g === b.g && a.b === b.b;
}

/** a swatch's name for a colour, or "Custom colour" for one from another app. */
export function colorName(color: Rgb | null, swatches: Swatch[]): string {
  if (!color) return "None";
  return swatches.find((swatch) => sameColor(swatch.color, color))?.name ?? "Custom colour";
}

export function rgbCss(color: Rgb, opacity = 1): string {
  return opacity >= 1 ? `rgb(${color.r} ${color.g} ${color.b})` : `rgb(${color.r} ${color.g} ${color.b} / ${opacity})`;
}

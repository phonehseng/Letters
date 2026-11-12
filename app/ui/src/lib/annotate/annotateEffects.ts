// the "what will happen" line under each Annotate tool's button, and the
// fonts a text box offers. like pageEffects.ts, the sentences say what
// changes, where, and how to take it back.

import { UNDO_HINT } from "../pageEffects";
import type { StandardFont, TextMarkupKind } from "./annotateApi";

const MARK_VERBS: Record<TextMarkupKind, { verb: string; noun: string }> = {
  highlight: { verb: "Highlights", noun: "highlight" },
  underline: { verb: "Underlines", noun: "underline" },
  strikeOut: { verb: "Strikes through", noun: "strike through" },
  squiggly: { verb: "Underlines with a wavy line", noun: "underline" },
};

/** "page 3" or "pages 2 to 4". */
export function pagesPhrase(pages: number[]): string {
  if (pages.length === 0) return "this page";
  const first = Math.min(...pages) + 1;
  const last = Math.max(...pages) + 1;
  return first === last ? `page ${first}` : `pages ${first} to ${last}`;
}

/**
 * the line under Highlight, Underline and Strike Through: what happens to
 * the selection, or how to use the tool when nothing is selected.
 */
export function markEffect(kind: TextMarkupKind, selectedPages: number[] | null, toolActive: boolean): string {
  const { verb, noun } = MARK_VERBS[kind];
  if (selectedPages && selectedPages.length > 0) {
    return `${verb} the selected text on ${pagesPhrase(selectedPages)}. ${UNDO_HINT}`;
  }
  if (toolActive) return `Drag across text on the page to ${noun} it. Press Escape when you are done.`;
  return `Select text on the page first, or click ${verbLabel(kind)} and then drag across text.`;
}

function verbLabel(kind: TextMarkupKind): string {
  return kind === "highlight" ? "Highlight" : kind === "underline" ? "Underline" : "Strike Through";
}

/** the line under a tool that is used by clicking or dragging on the page. */
export function placeEffect(what: string, how: string, toolActive: boolean): string {
  if (toolActive) return `${how} Press Escape when you are done.`;
  return `Adds ${what} to your working copy. ${UNDO_HINT}`;
}

/** the line under Flatten Comments. */
export function flattenEffect(count: number, pages: number[] | "all"): string {
  if (count === 0) return "There are no comments or marks to flatten here.";
  const where = pages === "all" ? "every page" : pagesPhrase(pages);
  const things = count === 1 ? "1 comment or mark" : `${count} comments and marks`;
  return `Paints ${things} on ${where} into the page. They can’t be edited afterwards, but Undo still works until you close the document.`;
}

// ---- fonts ----

export type FontFamily = "helvetica" | "times" | "courier";

export const FONT_FAMILIES: { value: FontFamily; label: string }[] = [
  { value: "helvetica", label: "Sans Serif" },
  { value: "times", label: "Serif" },
  { value: "courier", label: "Monospace" },
];

const FONTS: Record<FontFamily, [StandardFont, StandardFont, StandardFont, StandardFont]> = {
  // plain, bold, italic, bold italic
  helvetica: ["helvetica", "helveticaBold", "helveticaOblique", "helveticaBoldOblique"],
  times: ["timesRoman", "timesBold", "timesItalic", "timesBoldItalic"],
  courier: ["courier", "courierBold", "courierOblique", "courierBoldOblique"],
};

/** the engine's font for a family with bold and italic. */
export function standardFont(family: FontFamily, bold: boolean, italic: boolean): StandardFont {
  return FONTS[family][(bold ? 1 : 0) + (italic ? 2 : 0)];
}

/** a font taken apart into its family, bold and italic. */
export function fontParts(font: StandardFont): { family: FontFamily; bold: boolean; italic: boolean } {
  for (const family of Object.keys(FONTS) as FontFamily[]) {
    const index = FONTS[family].indexOf(font);
    if (index >= 0) return { family, bold: index % 2 === 1, italic: index >= 2 };
  }
  return { family: "helvetica", bold: false, italic: false };
}

/**
 * the css that draws a text box's font on screen while it is typed in,
 * so the box looks like the result. the pdf fonts themselves are not
 * installed, so the closest system fonts with the same widths stand in.
 */
export function cssFont(font: StandardFont): { family: string; weight: number; style: "normal" | "italic" } {
  const { family, bold, italic } = fontParts(font);
  const families: Record<FontFamily, string> = {
    helvetica: "Helvetica, Arial, sans-serif",
    times: '"Times New Roman", Times, serif',
    courier: '"Courier New", Courier, monospace',
  };
  return { family: families[family], weight: bold ? 700 : 400, style: italic ? "italic" : "normal" };
}

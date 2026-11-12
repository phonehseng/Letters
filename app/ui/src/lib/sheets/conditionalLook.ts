// how a cell is drawn once its conditional formats are applied: a
// matching rule's fill, text colour, bold, italic, underline and strike
// go over the cell's own, and a data bar sits under the text. the shell
// works out which rules match (core/sheets/src/conditional); this only
// decides what goes where, so the painter stays a list of drawing calls.

import type { DataBarLook, StyleLook, ViewCell } from "./sheetsApi";

export type Rect = { x: number; y: number; width: number; height: number };

// a cell with no style of its own is drawn as excel's default
const PLAIN: StyleLook = {
  bold: false,
  italic: false,
  underline: false,
  strike: false,
  size: null,
  font: null,
  wrap: false,
  vertical: "bottom",
  numberFormat: "General",
  borders: [null, null, null, null],
};

// excel keeps a data bar a little inside its cell, clear of the grid lines
const BAR_INSET = 2;

/** the fill drawn: a rule's over the cell's own; none leaves the paper */
export function fillOf(cell: ViewCell): string | undefined {
  return cell.conditional?.fill ?? cell.background;
}

/** the text colour drawn: a rule's over the number format's or font's */
export function textColorOf(cell: ViewCell): string | undefined {
  return cell.conditional?.color ?? cell.color;
}

/** the cell's style with a rule's bold, italic, underline and strike over it */
export function styleOf(cell: ViewCell, styles: StyleLook[]): StyleLook | undefined {
  const own = styles[cell.style];
  const rule = cell.conditional;
  if (!rule || (rule.bold === undefined && rule.italic === undefined && rule.underline === undefined && rule.strike === undefined)) return own;
  const base = own ?? PLAIN;
  return {
    ...base,
    bold: rule.bold ?? base.bold,
    italic: rule.italic ?? base.italic,
    underline: rule.underline ?? base.underline,
    strike: rule.strike ?? base.strike,
  };
}

/** whether the cell's text is drawn (a data bar can ask to show the bar alone) */
export function showsText(cell: ViewCell): boolean {
  return !cell.conditional?.bar?.hideValue;
}

export interface BarShape {
  bar: Rect;
  /** the axis's x, when bars grow from one inside the cell */
  axisX?: number;
}

/**
 * where a data bar goes in a cell: from the left edge, or from its axis
 * (rightwards for positive numbers, leftwards for negative ones).
 */
export function barShape(look: DataBarLook, cell: Rect, zoom: number): BarShape {
  const inset = BAR_INSET * zoom;
  const left = cell.x + inset;
  const room = Math.max(0, cell.width - inset * 2);
  const top = cell.y + inset;
  const height = Math.max(0, cell.height - inset * 2);
  const length = room * Math.min(1, Math.max(0, look.fraction));
  if (look.axis === undefined) return { bar: { x: left, y: top, width: length, height } };
  const axisX = left + room * look.axis;
  const x = look.negative ? axisX - length : axisX;
  return { bar: { x, y: top, width: length, height }, axisX };
}

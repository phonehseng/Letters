// the grid's selection, as excel has it: one or more ranges (ctrl-click
// adds one), the active cell where typing goes, and the anchor that
// shift keeps in place while the other corner moves. pure functions only,
// so every rule here is unit tested.

import { cellName, columnIndex, columnName } from "./geometry";
import type { CellRef, Range } from "./sheetsApi";

// the size of an excel sheet
export const MAX_ROWS = 1_048_576;
export const MAX_COLS = 16_384;

export interface Selection {
  ranges: Range[];
  active: CellRef;
  anchor: CellRef;
}

export function rangeOf(a: CellRef, b: CellRef): Range {
  return {
    start: { row: Math.min(a.row, b.row), col: Math.min(a.col, b.col) },
    end: { row: Math.max(a.row, b.row), col: Math.max(a.col, b.col) },
  };
}

export function single(cell: CellRef): Selection {
  return { ranges: [rangeOf(cell, cell)], active: cell, anchor: cell };
}

/** shift-click or shift with an arrow: the last range runs from the anchor to `cell`. */
export function extendTo(selection: Selection, cell: CellRef): Selection {
  const ranges = [...selection.ranges.slice(0, -1), rangeOf(selection.anchor, cell)];
  return { ranges, active: selection.active, anchor: selection.anchor };
}

/** ctrl-click: a new range beside the others. */
export function addCell(selection: Selection, cell: CellRef): Selection {
  return { ranges: [...selection.ranges, rangeOf(cell, cell)], active: cell, anchor: cell };
}

/** the corner that moves when shift extends, the one opposite the anchor. */
export function movingCorner(selection: Selection): CellRef {
  const last = selection.ranges[selection.ranges.length - 1];
  return {
    row: last.start.row === selection.anchor.row ? last.end.row : last.start.row,
    col: last.start.col === selection.anchor.col ? last.end.col : last.start.col,
  };
}

export function clampCell(cell: CellRef): CellRef {
  return {
    row: Math.min(MAX_ROWS - 1, Math.max(0, cell.row)),
    col: Math.min(MAX_COLS - 1, Math.max(0, cell.col)),
  };
}

export function wholeRows(first: number, last: number, activeCol = 0): Selection {
  const range = rangeOf({ row: first, col: 0 }, { row: last, col: MAX_COLS - 1 });
  const active = { row: first, col: activeCol };
  return { ranges: [range], active, anchor: { row: first, col: 0 } };
}

export function wholeColumns(first: number, last: number, activeRow = 0): Selection {
  const range = rangeOf({ row: 0, col: first }, { row: MAX_ROWS - 1, col: last });
  const active = { row: activeRow, col: first };
  return { ranges: [range], active, anchor: { row: 0, col: first } };
}

export function wholeSheet(): Selection {
  return { ranges: [rangeOf({ row: 0, col: 0 }, { row: MAX_ROWS - 1, col: MAX_COLS - 1 })], active: { row: 0, col: 0 }, anchor: { row: 0, col: 0 } };
}

export function isWholeColumns(range: Range): boolean {
  return range.start.row === 0 && range.end.row === MAX_ROWS - 1;
}

export function isWholeRows(range: Range): boolean {
  return range.start.col === 0 && range.end.col === MAX_COLS - 1;
}

export function contains(range: Range, row: number, col: number): boolean {
  return row >= range.start.row && row <= range.end.row && col >= range.start.col && col <= range.end.col;
}

export function isSingleCell(selection: Selection): boolean {
  const only = selection.ranges[0];
  return selection.ranges.length === 1 && only.start.row === only.end.row && only.start.col === only.end.col;
}

export function cellCount(range: Range): number {
  return (range.end.row - range.start.row + 1) * (range.end.col - range.start.col + 1);
}

/** "B3", "A1:C5", "A:B" or "2:4", as the name box shows a range. */
export function rangeName(range: Range): string {
  if (isWholeColumns(range) && !isWholeRows(range)) return `${columnName(range.start.col)}:${columnName(range.end.col)}`;
  if (isWholeRows(range) && !isWholeColumns(range)) return `${range.start.row + 1}:${range.end.row + 1}`;
  const start = cellName(range.start.row, range.start.col);
  if (range.start.row === range.end.row && range.start.col === range.end.col) return start;
  return `${start}:${cellName(range.end.row, range.end.col)}`;
}

/** reads "B3", "A1:C5", "C:C" or "4:9" (dollar signs allowed), or null. */
export function parseRangeName(text: string): Range | null {
  const plain = text.trim().replaceAll("$", "").toUpperCase();
  const [first, second = first] = plain.split(":");
  const cell = (part: string): CellRef | null => {
    const match = /^([A-Z]{1,3})(\d{1,7})$/.exec(part);
    if (!match) return null;
    const col = columnIndex(match[1]);
    const row = Number(match[2]) - 1;
    return col !== null && row >= 0 && row < MAX_ROWS && col < MAX_COLS ? { row, col } : null;
  };
  const a = cell(first);
  const b = cell(second);
  if (a && b) return rangeOf(a, b);
  const columns = [columnIndex(first), columnIndex(second)];
  if (columns[0] !== null && columns[1] !== null) return wholeColumns(Math.min(columns[0], columns[1]), Math.max(columns[0], columns[1])).ranges[0];
  if (/^\d+$/.test(first) && /^\d+$/.test(second)) {
    const rows = [Number(first) - 1, Number(second) - 1];
    if (rows.every((row) => row >= 0 && row < MAX_ROWS)) return wholeRows(Math.min(rows[0], rows[1]), Math.max(rows[0], rows[1])).ranges[0];
  }
  return null;
}

/**
 * Enter and Tab inside a selection of several cells move the active cell
 * through it (down the columns for Enter, along the rows for Tab) and wrap
 * at the end, as excel does; the selection stays.
 */
export function nextInSelection(selection: Selection, byRows: boolean, backwards: boolean): Selection {
  const range = selection.ranges[selection.ranges.length - 1];
  const step = backwards ? -1 : 1;
  let { row, col } = selection.active;
  if (byRows) {
    col += step;
    if (col > range.end.col || col < range.start.col) {
      col = backwards ? range.end.col : range.start.col;
      row += step;
      if (row > range.end.row || row < range.start.row) row = backwards ? range.end.row : range.start.row;
    }
  } else {
    row += step;
    if (row > range.end.row || row < range.start.row) {
      row = backwards ? range.end.row : range.start.row;
      col += step;
      if (col > range.end.col || col < range.start.col) col = backwards ? range.end.col : range.start.col;
    }
  }
  return { ...selection, active: { row, col } };
}

/** the ranges limited to the used part of the sheet (for copying and statistics). */
export function clampRange(range: Range, rows: number, cols: number): Range {
  return {
    start: range.start,
    end: { row: Math.min(range.end.row, Math.max(range.start.row, rows - 1)), col: Math.min(range.end.col, Math.max(range.start.col, cols - 1)) },
  };
}

// the commands that change more than the cells typed into: formats,
// inserting and deleting rows and columns, hiding, sorting, filtering,
// freezing panes, autosum and the sheet tabs. each is one undo step.

import { cellName, columnName } from "./geometry";
import type { SheetsContext } from "./gridActions";
import { startEditing } from "./gridActions";
import { clampRange, isWholeColumns, isWholeRows, rangeOf } from "./selection";
import { book, currentRegion, dataEdge, type Range, type StyleChange } from "./sheetsApi";

function lastRange(context: SheetsContext): Range {
  const ranges = context.view.selection.ranges;
  return ranges[ranges.length - 1];
}

/** the selection's style, toggled from the active cell's (bold on when it isn't bold). */
export function toggleStyle(context: SheetsContext, style: "bold" | "italic" | "underline" | "strike") {
  const { view, session } = context;
  const active = view.selection.active;
  const cell = view.cache.get(view.sheet, active.row, active.col);
  const look = cell ? view.styles[cell.style] : undefined;
  const on = !(look?.[style] ?? false);
  const label = { bold: "Bold", italic: "Italic", underline: "Underline", strike: "Strikethrough" }[style];
  void session.formatRanges(view.selection.ranges, { change: style, on }, label);
}

export function setFormat(context: SheetsContext, change: StyleChange, label: string) {
  void context.session.formatRanges(context.view.selection.ranges, change, label);
}

// --- rows and columns

/** the whole rows or columns the selection covers, as excel inserts and deletes them. */
function lines(range: Range, axis: "rows" | "columns"): { at: number; count: number } {
  return axis === "rows" ? { at: range.start.row, count: range.end.row - range.start.row + 1 } : { at: range.start.col, count: range.end.col - range.start.col + 1 };
}

export function insertLines(context: SheetsContext, axis: "rows" | "columns", after = false) {
  const { view, session } = context;
  const { at, count } = lines(lastRange(context), axis);
  const where = after ? at + count : at;
  const op = axis === "rows" ? "insertRows" : "insertColumns";
  const label = axis === "rows" ? (count === 1 ? "Insert Row" : "Insert Rows") : count === 1 ? "Insert Column" : "Insert Columns";
  void session.apply([book({ op, sheet: view.sheet, at: where, count })], label);
}

export function deleteLines(context: SheetsContext, axis: "rows" | "columns") {
  const { view, session } = context;
  const { at, count } = lines(lastRange(context), axis);
  const op = axis === "rows" ? "deleteRows" : "deleteColumns";
  void session.apply([book({ op, sheet: view.sheet, at, count })], axis === "rows" ? "Delete Rows" : "Delete Columns");
}

/** which kind of line ctrl - and ctrl shift + work on, or null to ask. */
export function selectedAxis(context: SheetsContext): "rows" | "columns" | null {
  const range = lastRange(context);
  if (isWholeRows(range)) return "rows";
  if (isWholeColumns(range)) return "columns";
  return null;
}

export function hideLines(context: SheetsContext, axis: "rows" | "columns", hidden: boolean) {
  const { session } = context;
  const range = lastRange(context);
  const first = axis === "rows" ? range.start.row : range.start.col;
  const last = Math.min(axis === "rows" ? range.end.row : range.end.col, first + 10_000);
  const indices = Array.from({ length: last - first + 1 }, (_, i) => first + i);
  void session.hide(axis, indices, hidden);
}

// --- sort and filter

/** the range a sort or filter works on: the selection, or the block of data around the cell. */
async function dataRange(context: SheetsContext): Promise<Range> {
  const { view, session } = context;
  const range = lastRange(context);
  if (range.start.row !== range.end.row || range.start.col !== range.end.col) return clampRange(range, view.usedRows(), view.usedCols());
  // inside a table the table is the data, its header row included
  const active = view.selection.active;
  const table = (view.info?.tables ?? []).find(([, r]) => active.row >= r.start.row && active.row <= r.end.row && active.col >= r.start.col && active.col <= r.end.col);
  if (table) return table[1];
  return currentRegion(session.id, view.sheet, view.selection.active);
}

/** excel's guess: a first row of text above a row with a number is a header. */
function looksLikeHeader(context: SheetsContext, range: Range): boolean {
  const { view } = context;
  if (range.start.row === range.end.row) return false;
  let textAbove = true;
  let numberBelow = false;
  for (let col = range.start.col; col <= range.end.col; col += 1) {
    const top = view.cache.get(view.sheet, range.start.row, col);
    const below = view.cache.get(view.sheet, range.start.row + 1, col);
    if (top && top.align === "right") textAbove = false;
    if (below && below.align === "right") numberBelow = true;
  }
  // a table's first row is always its header
  return (view.info?.tables ?? []).some(([, table]) => table.start.row === range.start.row) || (textAbove && numberBelow);
}

export async function sortBy(context: SheetsContext, descending: boolean) {
  const { view, session } = context;
  const range = await dataRange(context);
  const hasHeader = looksLikeHeader(context, range);
  const column = view.selection.active.col - range.start.col;
  const keys = [{ column: Math.max(0, column), descending }];
  const done = await session.apply([book({ op: "sort", sheet: view.sheet, range, keys, hasHeader })], descending ? "Sort Z to A" : "Sort A to Z");
  if (!done) return;
  const first = range.start.row + (hasHeader ? 1 : 0) + 1;
  const header = hasHeader ? "; the first row was kept as a header" : "";
  context.say(`Sorted rows ${first} to ${range.end.row + 1} by column ${columnName(view.selection.active.col)}${header}. Undo with Ctrl Z.`);
}

/** the range and column a filter menu should open for. */
export async function filterTarget(context: SheetsContext): Promise<{ range: Range; column: number }> {
  const { view } = context;
  const existing = view.info?.filter;
  const active = view.selection.active;
  if (existing && active.col >= existing.start.col && active.col <= existing.end.col) return { range: existing, column: active.col - existing.start.col };
  const range = await dataRange(context);
  return { range, column: Math.max(0, active.col - range.start.col) };
}

export function applyFilter(context: SheetsContext, range: Range, column: number, keep: string[]) {
  const { view, session } = context;
  void session.apply([book({ op: "filter", sheet: view.sheet, range, column, keep })], "Filter");
}

export function clearFilter(context: SheetsContext) {
  const { view, session } = context;
  void session.apply([book({ op: "clearFilter", sheet: view.sheet })], "Clear Filter");
}

// --- freezing

export type FreezeKind = "topRow" | "firstColumn" | "selection" | "none";

export function freeze(context: SheetsContext, kind: FreezeKind) {
  const { view, session } = context;
  const active = view.selection.active;
  const [rows, cols] = { topRow: [1, 0], firstColumn: [0, 1], selection: [active.row, active.col], none: [0, 0] }[kind];
  const label = kind === "none" ? "Unfreeze Panes" : "Freeze Panes";
  void session.apply([book({ op: "freeze", sheet: view.sheet, rows, cols })], label);
}

// --- autosum

/** alt =: starts =SUM( over the numbers above (or to the left), for Enter to confirm. */
export async function autoSum(context: SheetsContext) {
  const { view, session } = context;
  const at = view.selection.active;
  const above = at.row > 0 ? view.cache.get(view.sheet, at.row - 1, at.col) : undefined;
  const left = at.col > 0 ? view.cache.get(view.sheet, at.row, at.col - 1) : undefined;
  let range: Range | null = null;
  if (above?.text) {
    const top = await dataEdge(session.id, view.sheet, { row: at.row - 1, col: at.col }, "up");
    range = rangeOf(top, { row: at.row - 1, col: at.col });
  } else if (left?.text) {
    const start = await dataEdge(session.id, view.sheet, { row: at.row, col: at.col - 1 }, "left");
    range = rangeOf(start, { row: at.row, col: at.col - 1 });
  }
  const inside = range ? `${cellName(range.start.row, range.start.col)}:${cellName(range.end.row, range.end.col)}` : "";
  await startEditing(context, "enter", `=SUM(${inside})`);
  // the cursor goes before the closing bracket so a range can still be pointed at
  context.editor.update(context.editor.text, context.editor.text.length - 1);
}

// --- sheet tabs

export function addSheet(context: SheetsContext) {
  const { session } = context;
  const names = session.state.sheets.map((tab) => tab.name.toLowerCase());
  let number = session.state.sheets.length + 1;
  while (names.includes(`sheet${number}`)) number += 1;
  const index = session.activeSheet + 1;
  void session.apply([book({ op: "addSheet", index, name: `Sheet${number}` })], "Add Sheet").then((done) => {
    if (done) void showSheet(context, index);
  });
}

export async function showSheet(context: SheetsContext, index: number) {
  const { session, view, editor } = context;
  if (index < 0 || index >= session.state.sheets.length) return;
  if (editor.active && !editor.pointing) editor.end();
  session.activeSheet = index;
  await view.load(index);
}

export function renameSheet(context: SheetsContext, index: number, name: string) {
  void context.session.apply([book({ op: "renameSheet", sheet: index, name })], "Rename Sheet");
}

export function moveSheet(context: SheetsContext, from: number, to: number) {
  const { session } = context;
  if (to < 0 || to >= session.state.sheets.length) return;
  void session.apply([book({ op: "moveSheet", from, to })], "Move Sheet").then((done) => {
    if (done && session.activeSheet === from) void showSheet(context, to);
  });
}

export function duplicateSheet(context: SheetsContext, index: number) {
  void context.session.apply([book({ op: "duplicateSheet", sheet: index })], "Duplicate Sheet").then((done) => {
    if (done) void showSheet(context, index + 1);
  });
}

export function deleteSheet(context: SheetsContext, index: number) {
  const { session } = context;
  if (session.state.sheets.length <= 1) {
    context.say("A workbook needs at least one sheet.");
    return;
  }
  void session.apply([book({ op: "deleteSheet", sheet: index })], "Delete Sheet").then((done) => {
    if (done) void showSheet(context, Math.min(index, session.state.sheets.length - 1));
  });
}

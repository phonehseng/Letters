// what the grid's keys and pointer do: moving and extending the
// selection, typing into cells and finishing an entry, clearing, filling
// down and right, and the clipboard. the rules are excel's; the work is
// the engine's operations, sent through the session.

import type { CellEditor, EditMode } from "./cellEditor.svelte";
import type { CopiedRange } from "./clipboard";
import { pasteTargets, rowsFromHtml } from "./clipboard";
import { nowText } from "./formulaEntry";
import type { Direction } from "./gridKeys";
import { clampCell, clampRange, extendTo, isSingleCell, movingCorner, nextInSelection, rangeOf, single, type Selection } from "./selection";
import { book, copyCells, currentRegion, dataEdge, editText, type CellRef, type Edit, type Range } from "./sheetsApi";
import type { SheetsSession } from "./sheetsSession.svelte";
import type { SheetView } from "./sheetView.svelte";

export interface SheetsContext {
  session: SheetsSession;
  view: SheetView;
  editor: CellEditor;
  sheetName: () => string;
  /** how many rows and columns fit on screen, for page up and down */
  screenRows: () => number;
  screenCols: () => number;
  reveal: (cell: CellRef) => void;
  /** a short sentence for screen readers and the status bar */
  say: (text: string) => void;
  /** what the person just did, for the tips */
  recordTyped: (at: CellRef) => void;
}

const STEP: Record<Direction, [number, number]> = { up: [-1, 0], down: [1, 0], left: [0, -1], right: [0, 1] };

/** moves (or with `extend`, stretches) the selection one cell, skipping hidden lines. */
export function move(context: SheetsContext, direction: Direction, extend: boolean) {
  const { view } = context;
  const from = extend ? movingCorner(view.selection) : view.selection.active;
  const [down, across] = STEP[direction];
  const to = clampCell({
    row: down === 0 ? from.row : view.rows.nextVisible(from.row, down as 1 | -1),
    col: across === 0 ? from.col : view.cols.nextVisible(from.col, across as 1 | -1),
  });
  goTo(context, to, extend);
}

export function goTo(context: SheetsContext, to: CellRef, extend: boolean) {
  const { view } = context;
  view.select(extend ? extendTo(view.selection, to) : single(to));
  context.reveal(to);
}

/** ctrl with an arrow: the edge of the data, found by the shell. */
export async function jumpToEdge(context: SheetsContext, direction: Direction, extend: boolean) {
  const { view, session } = context;
  const from = extend ? movingCorner(view.selection) : view.selection.active;
  const to = await dataEdge(session.id, view.sheet, from, direction);
  goTo(context, to, extend);
}

export function page(context: SheetsContext, direction: Direction, extend: boolean) {
  const { view } = context;
  const from = extend ? movingCorner(view.selection) : view.selection.active;
  const [down, across] = STEP[direction];
  const to = clampCell({ row: from.row + down * context.screenRows(), col: from.col + across * context.screenCols() });
  goTo(context, to, extend);
}

/** Enter and Tab: inside a selection of several cells the active cell walks through it. */
export function step(context: SheetsContext, byRows: boolean, backwards: boolean) {
  const { view } = context;
  if (!isSingleCell(view.selection)) {
    view.select(nextInSelection(view.selection, byRows, backwards));
    context.reveal(view.selection.active);
    return;
  }
  move(context, byRows ? (backwards ? "left" : "right") : backwards ? "up" : "down", false);
}

/** ctrl a: the block of data first, the whole sheet when pressed again. */
export async function selectAll(context: SheetsContext) {
  const { view, session } = context;
  const region = await currentRegion(session.id, view.sheet, view.selection.active);
  const current = view.selection.ranges[0];
  const already = view.selection.ranges.length === 1 && JSON.stringify(current) === JSON.stringify(region);
  const single = region.start.row === region.end.row && region.start.col === region.end.col;
  if (already || single) {
    const all = rangeOf({ row: 0, col: 0 }, { row: 1_048_575, col: 16_383 });
    view.select({ ranges: [all], active: view.selection.active, anchor: { row: 0, col: 0 } });
    return;
  }
  view.select({ ranges: [region], active: view.selection.active, anchor: region.start });
}

// --- typing

/** starts typing into the active cell: `typed` replaces it (enter mode), or F2 edits it. */
export async function startEditing(context: SheetsContext, mode: EditMode, typed?: string) {
  const { view, editor, session } = context;
  const at = view.selection.active;
  const text = typed ?? (await editText(session.id, view.sheet, at.row, at.col));
  editor.begin(at, view.sheet, text, mode);
  context.reveal(at);
}

/** finishes the entry, then moves as the key that finished it says. */
export async function finishEditing(context: SheetsContext, then: Direction | null, everyCell = false) {
  const { view, editor, session } = context;
  if (!editor.active) return;
  const at = editor.at;
  const sheet = editor.sheet;
  const input = await editor.finished();
  // a click elsewhere may have moved the selection; the entry belongs to its own cell
  const active = view.selection.active;
  if (!everyCell && (active.row !== at.row || active.col !== at.col) && then) view.select(single(at));
  const edits: Edit[] = [book({ op: "setCell", sheet, row: at.row, col: at.col, input })];
  // ctrl enter puts the entry in every selected cell; formulas shift as a copy would
  if (everyCell) edits.push(...copiesOver(sheet, at, view.selection));
  const saved = await session.apply(edits, "Typing");
  if (saved) context.recordTyped(at);
  if (then) move(context, then, false);
}

function copiesOver(sheet: number, from: CellRef, selection: Selection): Edit[] {
  const edits: Edit[] = [];
  const source = rangeOf(from, from);
  for (const range of selection.ranges) {
    for (let row = range.start.row; row <= range.end.row && edits.length < 10_000; row += 1) {
      for (let col = range.start.col; col <= range.end.col; col += 1) {
        if (row === from.row && col === from.col) continue;
        edits.push(book({ op: "copy", sheet, from: source, toSheet: sheet, to: { row, col }, paste: "all" }));
      }
    }
  }
  return edits;
}

export function cancelEditing(context: SheetsContext) {
  context.editor.end();
}

/** Delete: empties the selected cells and keeps their formatting, as excel does. */
export function clearSelection(context: SheetsContext) {
  const { view, session } = context;
  const edits = view.selection.ranges.map((range) =>
    book({ op: "clear", sheet: view.sheet, range: clampRange(range, view.usedRows(), view.usedCols()), what: "contents" }),
  );
  void session.apply(edits, "Clear");
}

/** ctrl ; and ctrl shift ; : today's date or the time, typed in. */
export function typeNow(context: SheetsContext, what: "date" | "time") {
  const text = nowText(what);
  if (context.editor.active) {
    const { editor } = context;
    editor.update(editor.text.slice(0, editor.caret) + text + editor.text.slice(editor.caret), editor.caret + text.length);
    return;
  }
  void startEditing(context, "enter", text);
}

// --- filling

/** ctrl d and ctrl r: the first row (or column) of the selection copied down (or right). */
export function fillSelection(context: SheetsContext, direction: "down" | "right") {
  const { view, session } = context;
  const range = view.selection.ranges[view.selection.ranges.length - 1];
  const target = { ...range };
  let source: Range;
  if (direction === "down") {
    // one row selected: excel copies the cell above into it
    if (range.start.row === range.end.row) target.start = { ...target.start, row: Math.max(0, range.start.row - 1) };
    source = rangeOf(target.start, { row: target.start.row, col: target.end.col });
  } else {
    if (range.start.col === range.end.col) target.start = { ...target.start, col: Math.max(0, range.start.col - 1) };
    source = rangeOf(target.start, { row: target.end.row, col: target.start.col });
  }
  if (JSON.stringify(source) === JSON.stringify(target)) return;
  const copies = direction === "down" ? repeatDown(view.sheet, source, target) : repeatRight(view.sheet, source, target);
  // ctrl d copies and never makes a series; past this many copies the
  // engine's fill (one operation) is used, which copies formulas the same way
  const edits = copies.length <= MOST_COPIES ? copies : [book({ op: "fill", sheet: view.sheet, source, target })];
  void session.apply(edits, direction === "down" ? "Fill Down" : "Fill Right");
}

const MOST_COPIES = 10_000;
function repeatDown(sheet: number, source: Range, target: Range): Edit[] {
  const edits: Edit[] = [];
  for (let row = source.start.row + 1; row <= target.end.row; row += 1) edits.push(book({ op: "copy", sheet, from: source, toSheet: sheet, to: { row, col: source.start.col }, paste: "all" }));
  return edits;
}

function repeatRight(sheet: number, source: Range, target: Range): Edit[] {
  const edits: Edit[] = [];
  for (let col = source.start.col + 1; col <= target.end.col; col += 1) edits.push(book({ op: "copy", sheet, from: source, toSheet: sheet, to: { row: source.start.row, col }, paste: "all" }));
  return edits;
}

/** dragging the fill handle: a series where the cells make one, copies otherwise (the engine decides). */
export function fillByDrag(context: SheetsContext, source: Range, target: Range) {
  const { view, session } = context;
  void session.apply([book({ op: "fill", sheet: view.sheet, source, target })], "Fill").then((done) => {
    if (done) view.select({ ranges: [target], active: view.selection.active, anchor: target.start });
  });
}

// --- the clipboard

/** ctrl c and ctrl x: the cells as text and html, for letters and every other app. */
export async function copySelection(context: SheetsContext, cut: boolean): Promise<CopiedRange | null> {
  const { view, session } = context;
  if (view.selection.ranges.length > 1) {
    context.say("Copying several ranges at once isn’t possible. Select one range.");
    return null;
  }
  const range = clampRange(view.selection.ranges[0], view.usedRows(), view.usedCols());
  const copied = await copyCells(session.id, view.sheet, range);
  await writeClipboard(copied.text, copied.html);
  context.say(cut ? "Cut. Paste to move the cells." : "Copied.");
  return { sheet: view.sheet, range, text: copied.text, cut };
}

async function writeClipboard(text: string, html: string) {
  try {
    const item = new ClipboardItem({ "text/plain": new Blob([text], { type: "text/plain" }), "text/html": new Blob([html], { type: "text/html" }) });
    await navigator.clipboard.write([item]);
  } catch {
    await navigator.clipboard.writeText(text);
  }
}

/** a paste: letters' own copy moves or copies cells (formulas follow); anything else is typed in. */
export async function pasteData(context: SheetsContext, data: DataTransfer, ours: CopiedRange | null): Promise<"moved" | "pasted"> {
  const { view, session } = context;
  const text = data.getData("text/plain");
  const target = view.selection.ranges[view.selection.ranges.length - 1];
  if (ours && text === ours.text) {
    const op = ours.cut ? "move" : "copy";
    const places = ours.cut ? [target.start] : pasteTargets(ours.range, target);
    const edits = places.map((to) =>
      ours.cut ? book({ op, sheet: ours.sheet, from: ours.range, toSheet: view.sheet, to }) : book({ op, sheet: ours.sheet, from: ours.range, toSheet: view.sheet, to, paste: "all" }),
    );
    await session.apply(edits, ours.cut ? "Move" : "Paste");
    return ours.cut ? "moved" : "pasted";
  }
  const rows = rowsFromHtml(data.getData("text/html"));
  await session.paste(target.start, rows ? null : text, rows);
  return "pasted";
}

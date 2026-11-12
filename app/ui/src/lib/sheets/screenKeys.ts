// what each grid key does (gridKeys.ts says which key is which action),
// and the keys while a cell is being typed into: Enter, Tab and the
// arrows finish the entry or point at cells, Escape drops it, F4 cycles
// a reference's dollar signs, Alt Enter starts a new line in the cell.

import { cancelEditing, clearSelection, fillSelection, finishEditing, goTo, jumpToEdge, move, page, selectAll, startEditing, step, typeNow, type SheetsContext } from "./gridActions";
import type { Direction, GridAction } from "./gridKeys";
import { addSheet, autoSum, deleteLines, hideLines, insertLines, selectedAxis, setFormat, showSheet, toggleStyle } from "./sheetActions";
import { rangeOf, wholeColumns, wholeRows } from "./selection";

/** what the screen itself does for a few actions (menus and panels it owns). */
export interface ScreenHooks {
  copy: (cut: boolean) => void;
  find: (replace: boolean) => void;
  filter: () => void;
  formatMenu: () => void;
  /** ctrl - or ctrl shift + on a cell selection: ask rows or columns */
  askLines: (insert: boolean) => void;
}

export function runGridAction(context: SheetsContext, action: GridAction, hooks: ScreenHooks) {
  const { view } = context;
  const active = view.selection.active;
  switch (action.kind) {
    case "move":
      return move(context, action.direction, action.extend);
    case "edge":
      return void jumpToEdge(context, action.direction, action.extend);
    case "rowStart":
      return goTo(context, { row: active.row, col: view.info?.frozenCols ?? 0 }, action.extend);
    case "sheetStart":
      return goTo(context, { row: view.info?.frozenRows ?? 0, col: view.info?.frozenCols ?? 0 }, action.extend);
    case "sheetEnd":
      return goTo(context, { row: view.usedRows() - 1, col: view.usedCols() - 1 }, action.extend);
    case "page":
      return page(context, action.direction, action.extend);
    case "enter":
      return step(context, false, action.backwards);
    case "tab":
      return step(context, true, action.backwards);
    case "edit":
      return void startEditing(context, "edit");
    case "clear":
      return clearSelection(context);
    case "clearAndEdit":
      return void startEditing(context, "enter", "");
    default:
      return runOtherAction(context, action, hooks);
  }
}

function runOtherAction(context: SheetsContext, action: GridAction, hooks: ScreenHooks) {
  const { view } = context;
  const active = view.selection.active;
  switch (action.kind) {
    case "selectAll":
      return void selectAll(context);
    case "selectRow":
      return view.select(wholeRows(view.selection.ranges[0].start.row, view.selection.ranges[0].end.row, active.col));
    case "selectColumn":
      return view.select(wholeColumns(view.selection.ranges[0].start.col, view.selection.ranges[0].end.col, active.row));
    case "fill":
      return fillSelection(context, action.direction);
    case "toggle":
      return toggleStyle(context, action.style);
    case "numberFormat":
      return setFormat(context, { change: "numberFormat", code: action.code }, action.name);
    case "insertNow":
      return typeNow(context, action.what);
    case "copy":
      return hooks.copy(action.cut);
    case "autoSum":
      return void autoSum(context);
    case "deleteLines":
    case "insertLines":
      return linesByKey(context, action.kind === "insertLines", hooks);
    case "hide":
      return hideLines(context, action.axis, action.hidden);
    case "sheet":
      return void showSheet(context, context.session.activeSheet + action.step);
    case "addSheet":
      return addSheet(context);
    case "replace":
      return hooks.find(true);
    case "filter":
      return hooks.filter();
    case "formatCells":
      return hooks.formatMenu();
    case "zoom":
      return view.zoomStep(action.step);
    case "escape":
      return;
  }
}

function linesByKey(context: SheetsContext, insert: boolean, hooks: ScreenHooks) {
  const axis = selectedAxis(context);
  if (!axis) return hooks.askLines(insert);
  if (insert) insertLines(context, axis);
  else deleteLines(context, axis);
}

const ARROWS: Record<string, Direction> = { ArrowUp: "up", ArrowDown: "down", ArrowLeft: "left", ArrowRight: "right" };

/**
 * a key while typing into a cell. returns true when it was handled (and
 * the text field must not see it).
 */
export function editorKey(context: SheetsContext, event: KeyboardEvent): boolean {
  const { editor } = context;
  if (editor.completions.length > 0 && completionKey(context, event)) return true;
  if (event.key === "Enter" && event.altKey) return insertLineBreak(context);
  if (event.key === "Enter") {
    void finishEditing(context, event.shiftKey ? "up" : "down", event.ctrlKey || event.metaKey);
    return true;
  }
  if (event.key === "Tab") {
    void finishEditing(context, event.shiftKey ? "left" : "right");
    return true;
  }
  if (event.key === "Escape") {
    cancelEditing(context);
    return true;
  }
  if (event.key === "F4") {
    void editor.cycle();
    return true;
  }
  if (event.key === "F2") {
    editor.mode = editor.mode === "edit" ? "enter" : "edit";
    return true;
  }
  const arrow = ARROWS[event.key];
  if (arrow && editor.mode === "enter" && editor.place === "cell") return arrowWhileEntering(context, arrow, event.shiftKey);
  return false;
}

function completionKey(context: SheetsContext, event: KeyboardEvent): boolean {
  const { editor } = context;
  if (event.key === "Tab") {
    editor.acceptCompletion();
    return true;
  }
  if (event.key === "ArrowDown" || event.key === "ArrowUp") {
    editor.moveHighlight(event.key === "ArrowDown" ? 1 : -1);
    return true;
  }
  if (event.key === "Escape") {
    editor.completions = [];
    return true;
  }
  return false;
}

function insertLineBreak(context: SheetsContext): boolean {
  const { editor } = context;
  editor.update(`${editor.text.slice(0, editor.caret)}\n${editor.text.slice(editor.caret)}`, editor.caret + 1);
  return true;
}

/** in enter mode the arrows point at cells while a formula expects one, else finish and move */
function arrowWhileEntering(context: SheetsContext, arrow: Direction, extend: boolean): boolean {
  const { editor, view } = context;
  if (!editor.wantsReference()) {
    void finishEditing(context, arrow);
    return true;
  }
  const corner = editor.pointing?.to ?? editor.at;
  const [down, across] = { up: [-1, 0], down: [1, 0], left: [0, -1], right: [0, 1] }[arrow];
  const to = { row: Math.max(0, corner.row + down), col: Math.max(0, corner.col + across) };
  const anchor = extend && editor.pointing ? editor.pointing.from : to;
  editor.point(rangeOf(anchor, to), editor.sheet === view.sheet ? null : context.sheetName());
  if (editor.pointing) editor.pointing = { ...editor.pointing, from: anchor, to };
  context.reveal(to);
  return true;
}
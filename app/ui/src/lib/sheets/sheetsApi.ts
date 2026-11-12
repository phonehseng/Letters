// every call from the spreadsheet screen into the desktop shell (see
// app/shell/src/sheets_commands.rs and sheets_help_commands.rs), with the
// shapes of the answers, so components never need command names.
// rows and columns are zero based everywhere, as in the engine.

import { invoke } from "@tauri-apps/api/core";
import { save as showSaveDialog } from "@tauri-apps/plugin-dialog";

export interface CellRef {
  row: number;
  col: number;
}

export interface Range {
  start: CellRef;
  end: CellRef;
}

export interface SheetTab {
  name: string;
  hidden: boolean;
  color: string | null;
}

/** what the shell says after any change, and when a workbook opens */
export interface EditOutcome {
  revision: number;
  dirty: boolean;
  structure: boolean;
  sheets: SheetTab[];
  undoLabel: string | null;
  redoLabel: string | null;
  /** cells in a circular reference, as "Sheet1!A1" */
  circular: string[];
  label: string | null;
  calcMilliseconds: number;
}

export type SheetFormat = "xlsx" | "xlsm" | "ods" | "csv";

export interface OpenedSheet {
  id: number;
  name: string;
  path: string | null;
  format: SheetFormat;
  activeSheet: number;
  state: EditOutcome;
  recovery: { savedAt: number } | null;
  notes: string[];
  unknownFunctions: string[];
}

export interface SavedSheet {
  name: string;
  path: string;
  format: SheetFormat;
  state: EditOutcome;
}

export interface LineSize {
  first: number;
  last: number;
  /** characters for columns, points for rows; null is the default */
  size: number | null;
  hidden: boolean;
}

export interface Placeholder {
  kind: "chart" | "picture" | "shape";
  title: string;
  from: CellRef;
  to: CellRef;
}

export interface SheetInfo {
  usedRows: number;
  usedCols: number;
  defaultRowHeight: number;
  defaultColumnWidth: number;
  rows: LineSize[];
  columns: LineSize[];
  frozenRows: number;
  frozenCols: number;
  merged: Range[];
  placeholders: Placeholder[];
  filter: Range | null;
  tables: [string, Range][];
  showGridLines: boolean;
  activeCell: CellRef | null;
  protected: boolean;
}

export interface ViewCell {
  row: number;
  col: number;
  text: string;
  style: number;
  align: "left" | "center" | "right";
  color?: string;
  background?: string;
  isFormula?: boolean;
  notRecalculated?: boolean;
  hasNote?: boolean;
  hasLink?: boolean;
  merge?: Range;
}

export interface BorderLook {
  color: string;
  width: number;
  dashed: boolean;
}

export interface StyleLook {
  bold: boolean;
  italic: boolean;
  underline: boolean;
  strike: boolean;
  size: number | null;
  font: string | null;
  wrap: boolean;
  vertical: string;
  numberFormat: string;
  /** top, right, bottom, left */
  borders: [BorderLook | null, BorderLook | null, BorderLook | null, BorderLook | null];
}

/** one of the engine's edit operations, as json ({"op": "setCell", ...}) */
export type Operation = { op: string } & Record<string, unknown>;

export type Edit = { kind: "book"; edit: Operation } | { kind: "layout"; edit: unknown };

export function book(operation: Operation): Edit {
  return { kind: "book", edit: operation };
}

export type StyleChange =
  | { change: "numberFormat"; code: string }
  | { change: "bold" | "italic" | "underline" | "strike" | "wrap"; on: boolean }
  | { change: "align"; horizontal: string | null }
  | { change: "fill" | "fontColor"; color: string | null }
  | { change: "preset"; name: string };

export interface SelectionStats {
  count: number;
  numericCount: number;
  sum: number;
  average?: number;
  min?: number;
  max?: number;
  distinct: number;
}

export interface FindOptions {
  matchCase: boolean;
  wholeCell: boolean;
  inFormulas: boolean;
}

export interface Found {
  sheet: number;
  row: number;
  col: number;
  text: string;
}

// --- help (core/sheets help::*)

export interface Completion {
  name: string;
  description: string;
  matchedBy: "name" | "spelling" | "purpose";
}

export interface FunctionArgument {
  name: string;
  description: string;
  optional: boolean;
  repeating: boolean;
  kind: "range" | "number" | "text" | "condition" | "logical" | "any";
}

export interface FunctionInfo {
  name: string;
  category: string;
  description: string;
  arguments: FunctionArgument[];
  examples: { formula: string; result: string }[];
  mistakes: string[];
  related: string[];
}

export interface ArgumentHint {
  function: FunctionInfo;
  argument: number;
}

export interface ColouredReference {
  start: number;
  end: number;
  text: string;
  colour: string;
}

export interface Typed {
  text: string;
  caret: number;
}

export interface ExplainStep {
  depth: number;
  part: string;
  sentence: string;
  value?: string;
}

export interface ErrorFix {
  title: string;
  operation: Operation;
}

export interface ErrorHelp {
  error: string;
  meaning: string;
  why: string;
  fixes: ErrorFix[];
}

export interface Tip {
  id: string;
  title: string;
  why: string;
  preview: string;
  operations: Operation[];
}

export type RecentAction = { typed: { row: number; col: number } } | "other";

export interface Shortcut {
  operation: string;
  excelWindows: string;
  excelMac: string;
  googleSheets: string;
  description: string;
  conflict?: string;
  checked: boolean;
}

export interface FriendlyFormat {
  name: string;
  code: string;
  example: string;
}

// --- calls

export const openWorkbook = (path: string, recovered = false) => invoke<OpenedSheet>("sheets_open", { path, recovered });
export const newWorkbook = (template: string | null) => invoke<OpenedSheet>("sheets_new", { template });
export const closeWorkbook = (id: number) => invoke<void>("sheets_close", { id });
export const sheetInfo = (id: number, sheet: number) => invoke<SheetInfo>("sheets_info", { id, sheet });
export const sheetCells = (id: number, sheet: number, range: Range) => invoke<ViewCell[]>("sheets_cells", { id, sheet, range });
export const styleLooks = (id: number, from: number) => invoke<StyleLook[]>("sheets_styles", { id, from });
export const editText = (id: number, sheet: number, row: number, col: number) => invoke<string>("sheets_edit_text", { id, sheet, row, col });
export const applyEdits = (id: number, edits: Edit[], label: string) => invoke<EditOutcome>("sheets_apply", { id, edits, label });
export const formatCells = (id: number, sheet: number, ranges: Range[], change: StyleChange, label: string) =>
  invoke<EditOutcome>("sheets_format", { id, sheet, ranges, change, label });
export const resizeLines = (id: number, sheet: number, axis: "rows" | "columns", indices: number[], size: number | null) =>
  invoke<EditOutcome>("sheets_resize", { id, sheet, axis, indices, size });
export const hideLines = (id: number, sheet: number, axis: "rows" | "columns", indices: number[], hidden: boolean) =>
  invoke<EditOutcome>("sheets_hide", { id, sheet, axis, indices, hidden });
export const pasteText = (id: number, sheet: number, at: CellRef, text: string | null, rows: string[][] | null) =>
  invoke<EditOutcome>("sheets_paste_text", { id, sheet, row: at.row, col: at.col, text, rows });
export const copyCells = (id: number, sheet: number, range: Range) => invoke<{ text: string; html: string }>("sheets_copy", { id, sheet, range });
export const undoWorkbook = (id: number) => invoke<EditOutcome | null>("sheets_undo", { id });
export const redoWorkbook = (id: number) => invoke<EditOutcome | null>("sheets_redo", { id });
/** `replaceChanged`: the person chose to write over a change made to the file elsewhere. */
export const saveWorkbook = (id: number, path: string | null, sheet: number, replaceChanged = false) =>
  invoke<SavedSheet>("sheets_save", { id, path, sheet, replaceChanged });
export const saveLosses = (id: number, path: string) => invoke<string | null>("sheets_save_losses", { id, path });
export const autosaveWorkbook = (id: number) => invoke<void>("sheets_autosave", { id });
export const discardWorkbookRecovery = (id: number) => invoke<void>("sheets_discard_recovery", { id });
export const selectionStats = (id: number, sheet: number, ranges: Range[]) => invoke<SelectionStats | null>("sheets_stats", { id, sheet, ranges });
export const findCells = (id: number, sheet: number | null, text: string, options: FindOptions) => invoke<Found[]>("sheets_find", { id, sheet, text, options });
export const columnValues = (id: number, sheet: number, range: Range, column: number) => invoke<string[]>("sheets_column_values", { id, sheet, range, column });
export const dataEdge = (id: number, sheet: number, at: CellRef, direction: "up" | "down" | "left" | "right") =>
  invoke<CellRef>("sheets_edge", { id, sheet, at, direction });
export const currentRegion = (id: number, sheet: number, at: CellRef) => invoke<Range>("sheets_region", { id, sheet, at });

export const completeFunctions = (typed: string, limit: number) => invoke<Completion[]>("sheets_complete", { typed, limit });
export const functionInfo = (name: string) => invoke<FunctionInfo | null>("sheets_function_info", { name });
export const argumentHint = (formula: string, caret: number) => invoke<ArgumentHint | null>("sheets_argument_hint", { formula, caret });
export const referenceSpans = (formula: string) => invoke<ColouredReference[]>("sheets_reference_spans", { formula });
export const insertReference = (formula: string, caret: number, reference: string) => invoke<Typed>("sheets_insert_reference", { formula, caret, reference });
export const expectsReference = (formula: string, caret: number) => invoke<boolean>("sheets_expects_reference", { formula, caret });
export const cycleReference = (formula: string, caret: number) => invoke<Typed>("sheets_cycle_reference", { formula, caret });
export const autoClose = (formula: string) => invoke<string>("sheets_auto_close", { formula });
export const explainFormula = (id: number, sheet: number, at: CellRef, text: string) =>
  invoke<ExplainStep[]>("sheets_explain", { id, sheet, row: at.row, col: at.col, text });
export const diagnoseCell = (id: number, sheet: number, at: CellRef) => invoke<ErrorHelp | null>("sheets_diagnose", { id, sheet, row: at.row, col: at.col });
export const tipsFor = (id: number, sheet: number, selection: Range, recent: RecentAction[]) => invoke<Tip[]>("sheets_tips", { id, sheet, selection, recent });
export const dismissTip = (tip: string, wholeKind: boolean) => invoke<void>("sheets_dismiss_tip", { tip, wholeKind });
export const shortcutTable = () => invoke<Shortcut[]>("sheets_shortcuts");
export const friendlyFormats = () => invoke<FriendlyFormat[]>("sheets_friendly_formats");
export const stylePresets = () => invoke<string[]>("sheets_style_presets");

/** the save panel for Save As, offering the four formats Letters writes. */
export async function chooseWorkbookSaveLocation(suggested: string, format: SheetFormat): Promise<string | null> {
  const all = [
    { name: "Excel Workbook", extensions: ["xlsx"] },
    { name: "Excel Macro-Enabled Workbook", extensions: ["xlsm"] },
    { name: "OpenDocument Spreadsheet", extensions: ["ods"] },
    { name: "CSV (one sheet, values only)", extensions: ["csv"] },
  ];
  // the workbook's own format first, so Enter keeps it
  const filters = [...all.filter((f) => f.extensions[0] === format), ...all.filter((f) => f.extensions[0] !== format)];
  const chosen = await showSaveDialog({ defaultPath: suggested, filters });
  return typeof chosen === "string" ? chosen : null;
}

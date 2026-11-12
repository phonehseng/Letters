// one workbook while its window shows it: its name and file, whether it
// has unsaved changes, the sheet on screen, and every change sent to the
// shell one at a time (two quick edits must reach the workbook in the
// order they were made). the shell holds the workbook; this class only
// keeps what the screen shows about it.

import { asCommandError } from "../api";
import { createSerialQueue } from "../documentState";
import {
  applyEdits,
  autosaveWorkbook,
  chooseWorkbookSaveLocation,
  discardWorkbookRecovery,
  formatCells,
  hideLines,
  pasteText,
  redoWorkbook,
  resizeLines,
  saveLosses,
  saveWorkbook,
  undoWorkbook,
  type CellRef,
  type Edit,
  type EditOutcome,
  type OpenedSheet,
  type Range,
  type SheetFormat,
  type StyleChange,
} from "./sheetsApi";

/** recovery copies are asked for this often while there are unsaved changes, as the writer does */
export const AUTOSAVE_INTERVAL_MS = 5000;

export interface Notice {
  text: string;
  action?: { label: string; run: () => void };
}

export class SheetsSession {
  readonly id: number;
  name = $state("");
  path = $state<string | null>(null);
  format = $state<SheetFormat>("xlsx");
  state = $state<EditOutcome>() as EditOutcome;
  activeSheet = $state(0);
  notice = $state<Notice | null>(null);
  /** a change is on its way to the shell */
  busy = $state(false);
  private readonly queue = createSerialQueue();

  constructor(
    opened: OpenedSheet,
    private readonly onError: (message: string) => void,
    /** the grid redraws after each change */
    private readonly onChange: (outcome: EditOutcome) => void,
  ) {
    this.id = opened.id;
    this.name = opened.name;
    this.path = opened.path;
    this.format = opened.format;
    this.state = opened.state;
    this.activeSheet = opened.activeSheet;
  }

  /** runs one change after the ones before it; false when it failed (the message is shown). */
  private run(change: () => Promise<EditOutcome | null>): Promise<boolean> {
    return this.queue(async () => {
      this.busy = true;
      try {
        const outcome = await change();
        if (outcome) this.accept(outcome);
        return true;
      } catch (problem) {
        this.onError(asCommandError(problem).message);
        return false;
      } finally {
        this.busy = false;
      }
    });
  }

  accept(outcome: EditOutcome) {
    this.state = outcome;
    if (this.activeSheet >= outcome.sheets.length) this.activeSheet = outcome.sheets.length - 1;
    this.onChange(outcome);
  }

  apply(edits: Edit[], label: string): Promise<boolean> {
    if (edits.length === 0) return Promise.resolve(true);
    return this.run(() => applyEdits(this.id, edits, label));
  }

  formatRanges(ranges: Range[], change: StyleChange, label: string): Promise<boolean> {
    return this.run(() => formatCells(this.id, this.activeSheet, ranges, change, label));
  }

  resize(axis: "rows" | "columns", indices: number[], size: number | null): Promise<boolean> {
    return this.run(() => resizeLines(this.id, this.activeSheet, axis, indices, size));
  }

  hide(axis: "rows" | "columns", indices: number[], hidden: boolean): Promise<boolean> {
    return this.run(() => hideLines(this.id, this.activeSheet, axis, indices, hidden));
  }

  paste(at: CellRef, text: string | null, rows: string[][] | null): Promise<boolean> {
    return this.run(() => pasteText(this.id, this.activeSheet, at, text, rows));
  }

  undo(): Promise<boolean> {
    return this.run(() => undoWorkbook(this.id));
  }

  redo(): Promise<boolean> {
    return this.run(() => redoWorkbook(this.id));
  }

  /** saves to the file, or asks where for a new workbook. false when not saved. */
  async save(): Promise<boolean> {
    if (!this.path) return this.saveAs();
    // csv keeps one sheet: say so before the others are left out
    if (this.format === "csv" && this.state.sheets.length > 1) return this.askBeforeLosing(this.path);
    return this.writeTo(this.path);
  }

  async saveAs(): Promise<boolean> {
    const suggested = this.path ?? this.name;
    const chosen = await chooseWorkbookSaveLocation(suggested, this.format === "csv" ? "xlsx" : this.format);
    if (!chosen) return false;
    return this.askBeforeLosing(chosen);
  }

  private async askBeforeLosing(path: string): Promise<boolean> {
    const losses = await saveLosses(this.id, path).catch(() => null);
    if (!losses) return this.writeTo(path);
    this.notice = { text: losses, action: { label: "Save Anyway", run: () => void this.writeTo(path) } };
    return false;
  }

  private writeTo(path: string): Promise<boolean> {
    return this.queue(async () => {
      try {
        const saved = await saveWorkbook(this.id, path, this.activeSheet);
        this.name = saved.name;
        this.path = saved.path;
        this.format = saved.format;
        this.state = saved.state;
        return true;
      } catch (problem) {
        this.onError(asCommandError(problem).message);
        return false;
      }
    });
  }

  async autosave() {
    if (this.path && this.state.dirty) await autosaveWorkbook(this.id).catch(() => {});
  }

  async discardRecovery() {
    await discardWorkbookRecovery(this.id).catch(() => {});
  }
}

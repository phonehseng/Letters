// what the screen knows about the sheet on show: its sizes, frozen panes
// and features (from the shell's sheet info), the rows and columns the
// scroll area offers, the zoom, the styles' looks, the fetched cells and
// the selection. after every change in the workbook it reads what it
// needs again; the cells come back block by block as they are drawn.

import { CellCache } from "./cellCache";
import { columnLines, Lines, rowLines } from "./geometry";
import { MAX_COLS, MAX_ROWS, single, type Selection } from "./selection";
import { sheetCells, sheetInfo, styleLooks, type EditOutcome, type SheetInfo, type StyleLook } from "./sheetsApi";

// rows and columns offered past the last used cell, so there is room to
// type below and beside the data, as the end of a sheet has in excel
const EXTRA_ROWS = 200;
const EXTRA_COLS = 12;
// a sheet always offers at least a screenful: a to z, 1 to 100
const LEAST_COLS = 26;
const LEAST_ROWS = 100;

export const ZOOM_STEPS = [0.5, 0.75, 0.9, 1, 1.1, 1.25, 1.5, 2];

export class SheetView {
  info = $state<SheetInfo | null>(null);
  rows = $state<Lines>(new Lines(20, new Map(), LEAST_ROWS));
  cols = $state<Lines>(new Lines(64, new Map(), LEAST_COLS));
  selection = $state<Selection>(single({ row: 0, col: 0 }));
  zoom = $state(1);
  styles = $state<StyleLook[]>([]);
  /** goes up whenever something drawn changed, so the grid paints again */
  paints = $state(0);
  /** how far the scroll area reaches; grows when the selection goes further */
  private reachRows = 0;
  private reachCols = 0;
  readonly cache: CellCache;

  constructor(
    readonly id: number,
    public sheet: number,
  ) {
    this.cache = new CellCache(
      (sheet, range) => sheetCells(this.id, sheet, range),
      () => this.repaint(),
    );
  }

  repaint() {
    this.paints += 1;
  }

  /** reads the sheet's sizes and features (after opening, a sheet change or a structure edit). */
  async load(sheet = this.sheet) {
    const changedSheet = sheet !== this.sheet;
    this.sheet = sheet;
    const info = await sheetInfo(this.id, sheet);
    if (changedSheet) {
      this.cache.clear();
      this.reachRows = 0;
      this.reachCols = 0;
      this.selection = single(info.activeCell ?? { row: 0, col: 0 });
    }
    this.info = info;
    this.measure();
    await this.loadStyles();
  }

  /** builds the rows and columns from the sheet info, the zoom and the reach. */
  measure() {
    const info = this.info;
    if (!info) return;
    const active = this.selection.active;
    const rowCount = Math.min(MAX_ROWS, Math.max(LEAST_ROWS, info.usedRows + EXTRA_ROWS, active.row + EXTRA_ROWS, this.reachRows));
    const colCount = Math.min(MAX_COLS, Math.max(LEAST_COLS, info.usedCols + EXTRA_COLS, active.col + EXTRA_COLS, this.reachCols));
    this.reachRows = rowCount;
    this.reachCols = colCount;
    this.rows = rowLines(info.rows, info.defaultRowHeight, rowCount, this.zoom);
    this.cols = columnLines(info.columns, info.defaultColumnWidth, colCount, this.zoom);
    this.repaint();
  }

  /** scrolling near the end offers more rows, as excel's sheet goes on. */
  reachFurther(rows: boolean) {
    if (rows) this.reachRows = Math.min(MAX_ROWS, this.reachRows + EXTRA_ROWS * 5);
    else this.reachCols = Math.min(MAX_COLS, this.reachCols + EXTRA_COLS * 4);
    this.measure();
  }

  /** reads the used rows and columns again, keeping the rest of the sheet's info. */
  async refreshUsedSize() {
    const sheet = this.sheet;
    const info = await sheetInfo(this.id, sheet);
    if (sheet !== this.sheet || !this.info) return;
    if (info.usedRows === this.info.usedRows && info.usedCols === this.info.usedCols) return;
    this.info = { ...this.info, usedRows: info.usedRows, usedCols: info.usedCols };
    this.measure();
  }

  async loadStyles() {
    const more = await styleLooks(this.id, this.styles.length);
    if (more.length > 0) this.styles = [...this.styles, ...more];
  }

  /** the workbook changed: cells are stale, and sizes too after a structure change. */
  async afterChange(outcome: EditOutcome) {
    this.cache.changed(outcome.revision);
    if (outcome.structure) await this.load();
    else {
      // typing can grow the used part of the sheet (a value below the last
      // row); Delete, Copy, Cut, Ctrl End and sorting a selection clamp to
      // it, so a stale size cut new rows off (only A1 of A1:A3 was copied)
      await this.refreshUsedSize();
      await this.loadStyles();
    }
    this.repaint();
  }

  setZoom(zoom: number) {
    this.zoom = zoom;
    this.measure();
  }

  zoomStep(step: 1 | -1) {
    const index = ZOOM_STEPS.findIndex((zoom) => zoom >= this.zoom - 0.001);
    const next = ZOOM_STEPS[Math.min(ZOOM_STEPS.length - 1, Math.max(0, index + step))];
    this.setZoom(next);
  }

  select(selection: Selection) {
    this.selection = selection;
    const active = selection.active;
    if (active.row + EXTRA_ROWS > this.reachRows || active.col + EXTRA_COLS > this.reachCols) this.measure();
    this.repaint();
  }

  /** the used part of the sheet, for clamping whole rows and columns. */
  usedRows(): number {
    return this.info?.usedRows ?? 1;
  }

  usedCols(): number {
    return this.info?.usedCols ?? 1;
  }
}

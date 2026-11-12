// where things are on the grid's canvas: the headers, the frozen rows
// and columns that stay put, and the part that scrolls. one place turns
// a row or column into pixels and a pointer position back into a cell,
// so drawing and clicking can never disagree.

import type { Lines } from "./geometry";
import type { CellRef, Range } from "./sheetsApi";

export const COLUMN_HEADER_HEIGHT = 22;
// wide enough for "1048576" in the header font, with room on both sides
const PIXELS_PER_DIGIT = 7;
const ROW_HEADER_PADDING = 16;
const SMALLEST_ROW_HEADER = 40;

export function rowHeaderWidth(lastRow: number): number {
  return Math.max(SMALLEST_ROW_HEADER, String(lastRow + 1).length * PIXELS_PER_DIGIT + ROW_HEADER_PADDING);
}

/** one rectangle of the grid drawn with the same scroll: frozen or not, in each direction. */
export interface Region {
  firstRow: number;
  lastRow: number;
  firstCol: number;
  lastCol: number;
  /** the canvas rectangle the region may draw in */
  x: number;
  y: number;
  width: number;
  height: number;
}

export class GridLayout {
  readonly frozenWidth: number;
  readonly frozenHeight: number;

  constructor(
    readonly rows: Lines,
    readonly cols: Lines,
    readonly frozenRows: number,
    readonly frozenCols: number,
    readonly scrollX: number,
    readonly scrollY: number,
    readonly width: number,
    readonly height: number,
    readonly headerWidth: number,
  ) {
    this.frozenWidth = cols.offsetOf(frozenCols);
    this.frozenHeight = rows.offsetOf(frozenRows);
  }

  get headerHeight(): number {
    return COLUMN_HEADER_HEIGHT;
  }

  /** the canvas x where a column starts. */
  xOf(col: number): number {
    if (col < this.frozenCols) return this.headerWidth + this.cols.offsetOf(col);
    return this.headerWidth + this.cols.offsetOf(col) - this.scrollX;
  }

  yOf(row: number): number {
    if (row < this.frozenRows) return this.headerHeight + this.rows.offsetOf(row);
    return this.headerHeight + this.rows.offsetOf(row) - this.scrollY;
  }

  /**
   * where a row's bottom edge is drawn: its top plus its own height. the
   * top of the next row is no good for the last frozen row, because the
   * next row scrolls: scrolled down, that gave the frozen row a negative
   * height, and its number and cells weren't drawn (an empty band).
   */
  bottomOf(row: number): number {
    return this.yOf(row) + this.rows.sizeOf(row);
  }

  /** where a column's right edge is drawn (see bottomOf). */
  rightOf(col: number): number {
    return this.xOf(col) + this.cols.sizeOf(col);
  }

  /** the scrolled columns that show, first to last. */
  private scrolledCols(): [number, number] {
    const left = this.frozenWidth + this.scrollX;
    const visible = this.width - this.headerWidth - this.frozenWidth;
    return [Math.max(this.frozenCols, this.cols.indexAt(left)), this.cols.indexAt(left + visible)];
  }

  private scrolledRows(): [number, number] {
    const top = this.frozenHeight + this.scrollY;
    const visible = this.height - this.headerHeight - this.frozenHeight;
    return [Math.max(this.frozenRows, this.rows.indexAt(top)), this.rows.indexAt(top + visible)];
  }

  /** the four regions (fewer without frozen panes), main region first. */
  regions(): Region[] {
    const [firstCol, lastCol] = this.scrolledCols();
    const [firstRow, lastRow] = this.scrolledRows();
    const left = this.headerWidth + this.frozenWidth;
    const top = this.headerHeight + this.frozenHeight;
    const across = { x: left, width: this.width - left };
    const down = { y: top, height: this.height - top };
    const regions: Region[] = [{ firstRow, lastRow, firstCol, lastCol, ...across, ...down }];
    const frozenAcross = { firstCol: 0, lastCol: this.frozenCols - 1, x: this.headerWidth, width: this.frozenWidth };
    const frozenDown = { firstRow: 0, lastRow: this.frozenRows - 1, y: this.headerHeight, height: this.frozenHeight };
    if (this.frozenCols > 0) regions.push({ firstRow, lastRow, ...frozenAcross, ...down });
    if (this.frozenRows > 0) regions.push({ firstCol, lastCol, ...across, ...frozenDown });
    if (this.frozenRows > 0 && this.frozenCols > 0) regions.push({ ...frozenAcross, ...frozenDown });
    return regions;
  }

  /**
   * the cells on screen, one range per region, for asking the shell for
   * them. one range from the frozen rows down to the last row shown asked
   * for every row between (about 2,300 blocks near row 150,000), and the
   * cache, which keeps the newest blocks, then dropped the frozen rows':
   * scrolled down, a frozen top row showed as an empty band.
   */
  visibleRanges(): Range[] {
    return this.regions()
      .filter((region) => region.lastRow >= region.firstRow && region.lastCol >= region.firstCol)
      .map((region) => ({ start: { row: region.firstRow, col: region.firstCol }, end: { row: region.lastRow, col: region.lastCol } }));
  }

  /** the cell under a canvas point (headers give row or column -1). */
  cellAt(x: number, y: number): CellRef {
    const col = x < this.headerWidth ? -1 : this.colAt(x);
    const row = y < this.headerHeight ? -1 : this.rowAt(y);
    return { row, col };
  }

  private colAt(x: number): number {
    const inside = x - this.headerWidth;
    if (inside < this.frozenWidth) return this.cols.indexAt(inside);
    return this.cols.indexAt(inside + this.scrollX);
  }

  private rowAt(y: number): number {
    const inside = y - this.headerHeight;
    if (inside < this.frozenHeight) return this.rows.indexAt(inside);
    return this.rows.indexAt(inside + this.scrollY);
  }

  /** the rectangle of a range on the canvas (it may run off the edges). */
  rectOf(range: Range): { x: number; y: number; width: number; height: number } {
    const x = this.xOf(range.start.col);
    const y = this.yOf(range.start.row);
    return { x, y, width: this.rightOf(range.end.col) - x, height: this.bottomOf(range.end.row) - y };
  }

  /** the scroll position that brings a cell fully into view (frozen cells need none). */
  scrollToShow(cell: CellRef): { x: number; y: number } {
    let { scrollX: x, scrollY: y } = this;
    if (cell.col >= this.frozenCols) {
      const start = this.cols.offsetOf(cell.col) - this.frozenWidth;
      const end = this.cols.offsetOf(cell.col + 1) - this.frozenWidth;
      const room = this.width - this.headerWidth - this.frozenWidth;
      if (start < x) x = start;
      else if (end > x + room) x = Math.min(start, end - room);
    }
    if (cell.row >= this.frozenRows) {
      const start = this.rows.offsetOf(cell.row) - this.frozenHeight;
      const end = this.rows.offsetOf(cell.row + 1) - this.frozenHeight;
      const room = this.height - this.headerHeight - this.frozenHeight;
      if (start < y) y = start;
      else if (end > y + room) y = Math.min(start, end - room);
    }
    return { x: Math.max(0, x), y: Math.max(0, y) };
  }
}

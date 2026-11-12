// the cells the grid has fetched from the shell, in blocks of rows and
// columns. scrolling asks for the blocks on screen; a block arrives once
// and is kept until the workbook changes. after an edit the old blocks
// stay on screen until their fresh copies arrive, so nothing flickers.

import type { Range, ViewCell } from "./sheetsApi";

// 64 rows by 32 columns: about three screens of a laptop's height per
// block, and a fetch of one block takes well under a millisecond in the
// shell (measured on 200,000 rows), so a screen needs one to four of them
export const BLOCK_ROWS = 64;
export const BLOCK_COLS = 32;
// cells are kept by row and column in one number; 16,384 columns fit
const COLUMNS_PER_ROW = 16_384;
// blocks kept for scrolling back; beyond this the oldest are dropped
const MOST_BLOCKS = 400;

interface Block {
  cells: Map<number, ViewCell>;
  revision: number;
}

export type FetchCells = (sheet: number, range: Range) => Promise<ViewCell[]>;

export function cellKey(row: number, col: number): number {
  return row * COLUMNS_PER_ROW + col;
}

export class CellCache {
  private blocks = new Map<string, Block>();
  private pending = new Set<string>();
  /** the workbook's revision the cells should match */
  revision = 0;

  constructor(
    private readonly fetch: FetchCells,
    /** called when a block arrives, so the grid paints again */
    private readonly onArrive: () => void,
  ) {}

  private key(sheet: number, blockRow: number, blockCol: number): string {
    return `${sheet}:${blockRow}:${blockCol}`;
  }

  /** the cell, as last fetched (possibly from before the latest edit). */
  get(sheet: number, row: number, col: number): ViewCell | undefined {
    const block = this.blocks.get(this.key(sheet, Math.floor(row / BLOCK_ROWS), Math.floor(col / BLOCK_COLS)));
    return block?.cells.get(cellKey(row, col));
  }

  /** whether every block of a range is here and current. */
  isCurrent(sheet: number, range: Range): boolean {
    return this.blocksOf(range).every(([r, c]) => this.blocks.get(this.key(sheet, r, c))?.revision === this.revision);
  }

  /** asks for the blocks of `range` that are missing or stale. */
  want(sheet: number, range: Range): Promise<void> {
    const waits: Promise<void>[] = [];
    for (const [blockRow, blockCol] of this.blocksOf(range)) {
      const key = this.key(sheet, blockRow, blockCol);
      if (this.blocks.get(key)?.revision === this.revision || this.pending.has(key)) continue;
      waits.push(this.load(sheet, blockRow, blockCol, key));
    }
    return Promise.all(waits).then(() => undefined);
  }

  private async load(sheet: number, blockRow: number, blockCol: number, key: string) {
    const revision = this.revision;
    this.pending.add(key);
    const range = {
      start: { row: blockRow * BLOCK_ROWS, col: blockCol * BLOCK_COLS },
      end: { row: blockRow * BLOCK_ROWS + BLOCK_ROWS - 1, col: blockCol * BLOCK_COLS + BLOCK_COLS - 1 },
    };
    try {
      const cells = await this.fetch(sheet, range);
      const map = new Map<number, ViewCell>();
      for (const cell of cells) map.set(cellKey(cell.row, cell.col), cell);
      // an answer for an older revision still beats nothing, but stays stale
      this.blocks.delete(key);
      this.blocks.set(key, { cells: map, revision });
      this.forgetOldest();
    } finally {
      this.pending.delete(key);
    }
    this.onArrive();
  }

  private forgetOldest() {
    while (this.blocks.size > MOST_BLOCKS) {
      const oldest = this.blocks.keys().next().value;
      if (oldest === undefined) return;
      this.blocks.delete(oldest);
    }
  }

  private blocksOf(range: Range): [number, number][] {
    const blocks: [number, number][] = [];
    for (let r = Math.floor(range.start.row / BLOCK_ROWS); r <= Math.floor(range.end.row / BLOCK_ROWS); r += 1) {
      for (let c = Math.floor(range.start.col / BLOCK_COLS); c <= Math.floor(range.end.col / BLOCK_COLS); c += 1) blocks.push([r, c]);
    }
    return blocks;
  }

  /** the workbook changed: every block is stale, but kept until replaced. */
  changed(revision: number) {
    this.revision = revision;
  }

  /** another sheet's blocks, or a structure change: drop everything. */
  clear() {
    this.blocks.clear();
  }
}

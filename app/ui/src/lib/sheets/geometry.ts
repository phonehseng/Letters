// where each row and column sits on screen. a sheet has a default size
// and a few lines that differ (wider columns, taller or hidden rows), so
// positions are worked out from the default plus the differences before
// a line, never by adding up 200,000 heights.

import type { LineSize } from "./sheetsApi";

// excel draws a column of n characters of calibri 11 as 7 pixels per
// character plus 5 pixels of padding: the default 8.43 is 64 pixels
const PIXELS_PER_CHARACTER = 7;
const COLUMN_PADDING_PIXELS = 5;
// rows are measured in points; the screen has 96 pixels to 72 points
const PIXELS_PER_POINT = 96 / 72;

export function columnPixels(characters: number): number {
  return Math.round(characters * PIXELS_PER_CHARACTER + COLUMN_PADDING_PIXELS);
}

export function columnCharacters(pixels: number): number {
  const characters = Math.max(0, pixels - COLUMN_PADDING_PIXELS) / PIXELS_PER_CHARACTER;
  return Math.round(characters * 100) / 100;
}

export function rowPixels(points: number): number {
  return Math.round(points * PIXELS_PER_POINT);
}

export function rowPoints(pixels: number): number {
  return Math.round((pixels / PIXELS_PER_POINT) * 4) / 4;
}

/** the rows or the columns of one sheet. */
export class Lines {
  /** lines that differ from the default, sorted, with their sizes */
  private readonly exceptions: number[] = [];
  private readonly sizes = new Map<number, number>();
  /** the sum of (size - default) of every exception before index i of `exceptions` */
  private readonly deltasBefore: number[] = [];

  constructor(
    readonly defaultSize: number,
    differences: Map<number, number>,
    /** how many lines the scroll area offers */
    public count: number,
  ) {
    this.exceptions = [...differences.keys()].sort((a, b) => a - b);
    let running = 0;
    for (const index of this.exceptions) {
      const size = differences.get(index) ?? defaultSize;
      this.sizes.set(index, size);
      this.deltasBefore.push(running);
      running += size - defaultSize;
    }
    this.deltasBefore.push(running);
  }

  sizeOf(index: number): number {
    return this.sizes.get(index) ?? this.defaultSize;
  }

  isHidden(index: number): boolean {
    return this.sizeOf(index) === 0;
  }

  /** how many exceptions come before `index` */
  private exceptionsBefore(index: number): number {
    let low = 0;
    let high = this.exceptions.length;
    while (low < high) {
      const middle = (low + high) >> 1;
      if (this.exceptions[middle] < index) low = middle + 1;
      else high = middle;
    }
    return low;
  }

  /** the pixel where line `index` starts */
  offsetOf(index: number): number {
    return index * this.defaultSize + this.deltasBefore[this.exceptionsBefore(index)];
  }

  /** the whole length of the scroll area */
  total(): number {
    return this.offsetOf(this.count);
  }

  /** the line at pixel `offset` (the last one past the end) */
  indexAt(offset: number): number {
    let low = 0;
    let high = this.count - 1;
    while (low < high) {
      const middle = (low + high + 1) >> 1;
      if (this.offsetOf(middle) <= offset) low = middle;
      else high = middle - 1;
    }
    return Math.max(0, low);
  }

  /** the next line in a direction that isn't hidden, or `index` itself at the edge */
  nextVisible(index: number, step: 1 | -1): number {
    let next = index + step;
    while (next >= 0 && next < this.count && this.isHidden(next)) next += step;
    return next >= 0 && next < this.count ? next : index;
  }
}

/** a sheet's columns from the shell's answer: characters become pixels. */
export function columnLines(sizes: LineSize[], defaultCharacters: number, count: number, zoom: number): Lines {
  const differences = new Map<number, number>();
  for (const run of sizes) {
    const pixels = run.hidden ? 0 : columnPixels(run.size ?? defaultCharacters) * zoom;
    // a run as wide as the sheet is listed line by line only as far as shown
    for (let col = run.first; col <= Math.min(run.last, count); col += 1) differences.set(col, Math.round(pixels));
  }
  return new Lines(Math.round(columnPixels(defaultCharacters) * zoom), differences, count);
}

/** a sheet's rows from the shell's answer: points become pixels. */
export function rowLines(sizes: LineSize[], defaultPoints: number, count: number, zoom: number): Lines {
  const differences = new Map<number, number>();
  for (const line of sizes) {
    const pixels = line.hidden ? 0 : rowPixels(line.size ?? defaultPoints) * zoom;
    for (let row = line.first; row <= line.last; row += 1) differences.set(row, Math.round(pixels));
  }
  return new Lines(Math.round(rowPixels(defaultPoints) * zoom), differences, count);
}

/** "A", "Z", "AA", "XFD" for a zero-based column. */
export function columnName(col: number): string {
  let name = "";
  let rest = col + 1;
  while (rest > 0) {
    const letter = (rest - 1) % 26;
    name = String.fromCharCode(65 + letter) + name;
    rest = Math.floor((rest - 1) / 26);
  }
  return name;
}

/** "B3" for a zero-based cell; with `absolute`, "$B$3". */
export function cellName(row: number, col: number, absolute = false): string {
  const dollar = absolute ? "$" : "";
  return `${dollar}${columnName(col)}${dollar}${row + 1}`;
}

/** the column index of letters such as "AB", or null. */
export function columnIndex(letters: string): number | null {
  if (!/^[A-Za-z]{1,3}$/.test(letters)) return null;
  let index = 0;
  for (const letter of letters.toUpperCase()) index = index * 26 + (letter.charCodeAt(0) - 64);
  return index - 1;
}

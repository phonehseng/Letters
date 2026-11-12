// the changes list is virtualised: a 500 page pair can have thousands of
// changes, and only the rows in view are made. rows have fixed heights
// (a heading row and a change row of two lines), so finding them is
// arithmetic, not measuring.

import type { ListRow } from "./changes";

/** in rem, so the list follows the text size; turned into pixels by the caller. */
export const HEADING_ROW_REM = 2;
export const CHANGE_ROW_REM = 3.5;

export interface RowSpot {
  index: number;
  top: number;
  height: number;
}

/** where each row starts, in pixels, and the list's whole height. */
export function rowTops(rows: ListRow[], remPx: number): { tops: number[]; total: number } {
  const tops: number[] = [];
  let top = 0;
  for (const row of rows) {
    tops.push(top);
    top += rowHeight(row, remPx);
  }
  return { tops, total: top };
}

export function rowHeight(row: ListRow, remPx: number): number {
  return (row.type === "heading" ? HEADING_ROW_REM : CHANGE_ROW_REM) * remPx;
}

/** the rows that overlap the visible band, plus `extra` pixels either side. */
export function visibleRows(rows: ListRow[], tops: number[], remPx: number, scrollTop: number, height: number, extra = 200): RowSpot[] {
  if (rows.length === 0) return [];
  const start = scrollTop - extra;
  const end = scrollTop + height + extra;
  // binary search for the first row ending after `start`
  let low = 0;
  let high = rows.length;
  while (low < high) {
    const middle = (low + high) >> 1;
    if (tops[middle] + rowHeight(rows[middle], remPx) <= start) low = middle + 1;
    else high = middle;
  }
  const spots: RowSpot[] = [];
  for (let index = low; index < rows.length && tops[index] < end; index += 1) {
    spots.push({ index, top: tops[index], height: rowHeight(rows[index], remPx) });
  }
  return spots;
}

/** the scroll that brings a row fully into view, or null when it already is. */
export function scrollToRow(top: number, height: number, scrollTop: number, viewport: number): number | null {
  if (top >= scrollTop && top + height <= scrollTop + viewport) return null;
  if (top < scrollTop) return top;
  return Math.max(0, top + height - viewport);
}

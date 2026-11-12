// what a press on the grid's canvas lands on: a cell, a row or column
// header, the edge between two headers (to resize), the fill handle, a
// filter button or the corner that selects everything. one place
// decides it, so the cursor shape and the press always agree.

import type { GridLayout } from "./gridLayout";
import { FILL_HANDLE, fillHandleAt, filterButtonAt } from "./overlays";
import type { Range } from "./sheetsApi";

// how close to a header edge the pointer must be to resize instead of select
const EDGE_REACH = 4;

export type Hit =
  | { kind: "cell"; row: number; col: number }
  | { kind: "rowHeader"; row: number }
  | { kind: "columnHeader"; col: number }
  | { kind: "corner" }
  | { kind: "columnEdge"; col: number }
  | { kind: "rowEdge"; row: number }
  | { kind: "fillHandle" }
  | { kind: "filterButton"; col: number };

/** the line whose far edge is within reach of `at`, or null. */
function edgeNear(at: number, position: (index: number) => number, index: number): number | null {
  if (Math.abs(position(index + 1) - at) <= EDGE_REACH) return index;
  if (index > 0 && Math.abs(position(index) - at) <= EDGE_REACH) return index - 1;
  return null;
}

export function hitTest(layout: GridLayout, x: number, y: number, selected: Range | null, filter: Range | null): Hit {
  const { row, col } = layout.cellAt(x, y);
  if (row < 0 && col < 0) return { kind: "corner" };
  if (row < 0) {
    const edge = edgeNear(x, (index) => layout.xOf(index), col);
    return edge !== null ? { kind: "columnEdge", col: edge } : { kind: "columnHeader", col };
  }
  if (col < 0) {
    const edge = edgeNear(y, (index) => layout.yOf(index), row);
    return edge !== null ? { kind: "rowEdge", row: edge } : { kind: "rowHeader", row };
  }
  if (selected) {
    const handle = fillHandleAt(layout, selected);
    const reach = FILL_HANDLE + 2;
    if (x >= handle.x - 2 && x <= handle.x + reach && y >= handle.y - 2 && y <= handle.y + reach) return { kind: "fillHandle" };
  }
  const filterCol = filterButtonAt(layout, filter, x, y);
  if (filterCol !== null) return { kind: "filterButton", col: filterCol };
  return { kind: "cell", row, col };
}

/** the css cursor for what is under the pointer. */
export function cursorFor(hit: Hit): string {
  switch (hit.kind) {
    case "columnEdge":
      return "col-resize";
    case "rowEdge":
      return "row-resize";
    case "fillHandle":
      return "crosshair";
    case "filterButton":
      return "pointer";
    case "rowHeader":
      return "e-resize";
    case "columnHeader":
      return "s-resize";
    default:
      return "cell";
  }
}

/**
 * where dragging the fill handle from `source` to `over` fills: along the
 * one direction the pointer went furthest, as excel fills only down,
 * up, left or right.
 */
export function fillTarget(source: Range, over: { row: number; col: number }): Range | null {
  const below = over.row - source.end.row;
  const above = source.start.row - over.row;
  const right = over.col - source.end.col;
  const left = source.start.col - over.col;
  const furthest = Math.max(below, above, right, left);
  if (furthest <= 0) return null;
  if (furthest === below) return { start: source.start, end: { row: over.row, col: source.end.col } };
  if (furthest === above) return { start: { row: over.row, col: source.start.col }, end: source.end };
  if (furthest === right) return { start: source.start, end: { row: source.end.row, col: over.col } };
  return { start: { row: source.start.row, col: over.col }, end: source.end };
}

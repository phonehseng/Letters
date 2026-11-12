// what is drawn over the cells: the selection with its fill handle, the
// coloured outlines of the references in a formula being typed, the
// dashed border of a copied range, filter buttons, chart placeholders,
// the lines of frozen panes, and the row and column headers. colours of
// the app's chrome come from its css tokens, so they follow dark mode.

import { columnName } from "./geometry";
import type { GridLayout } from "./gridLayout";
import { isWholeColumns, isWholeRows, type Selection } from "./selection";
import type { Placeholder, Range } from "./sheetsApi";

export const FILL_HANDLE = 6;
const FILTER_BUTTON = 14;

export interface Theme {
  accent: string;
  accentSoft: string;
  header: string;
  headerText: string;
  headerLine: string;
  headerSelected: string;
  headerFont: string;
}

export interface Overlay {
  selection: Selection;
  references: { range: Range; colour: string }[];
  copied: Range | null;
  fillPreview: Range | null;
  filter: Range | null;
  placeholders: Placeholder[];
  /** the cell being typed into hides the selection's fill there */
  editing: boolean;
}

/** a rectangle clipped to the cell area, or null when it is off screen. */
function visibleRect(layout: GridLayout, range: Range) {
  const rect = layout.rectOf(range);
  const left = Math.max(rect.x, layout.headerWidth);
  const top = Math.max(rect.y, layout.headerHeight);
  const right = Math.min(rect.x + rect.width, layout.width);
  const bottom = Math.min(rect.y + rect.height, layout.height);
  if (right <= left || bottom <= top) return null;
  return { x: left, y: top, width: right - left, height: bottom - top, whole: rect };
}

export function paintOverlay(ctx: CanvasRenderingContext2D, layout: GridLayout, overlay: Overlay, theme: Theme) {
  ctx.save();
  ctx.beginPath();
  ctx.rect(layout.headerWidth, layout.headerHeight, layout.width, layout.height);
  ctx.clip();
  paintPlaceholders(ctx, layout, overlay.placeholders);
  paintFrozenLines(ctx, layout);
  paintSelection(ctx, layout, overlay, theme);
  paintReferences(ctx, layout, overlay.references);
  if (overlay.copied) paintDashed(ctx, layout, overlay.copied, theme.accent);
  if (overlay.fillPreview) paintDashed(ctx, layout, overlay.fillPreview, "#7f7f7f");
  if (overlay.filter) paintFilterButtons(ctx, layout, overlay.filter, theme);
  ctx.restore();
}

function paintSelection(ctx: CanvasRenderingContext2D, layout: GridLayout, overlay: Overlay, theme: Theme) {
  const { selection } = overlay;
  ctx.fillStyle = theme.accentSoft;
  // the active cell is left unshaded, as in excel, so its value reads plainly
  const active = layout.rectOf({ start: selection.active, end: selection.active });
  for (const range of selection.ranges) {
    const rect = visibleRect(layout, range);
    if (rect) fillAround(ctx, rect, active);
  }
  const last = selection.ranges[selection.ranges.length - 1];
  const rect = visibleRect(layout, last);
  if (!rect) return;
  ctx.strokeStyle = theme.accent;
  ctx.lineWidth = 2;
  ctx.strokeRect(rect.whole.x, rect.whole.y, rect.whole.width - 1, rect.whole.height - 1);
  if (overlay.editing || selection.ranges.length > 1) return;
  ctx.fillStyle = theme.accent;
  const handle = fillHandleAt(layout, last);
  ctx.fillRect(handle.x, handle.y, FILL_HANDLE, FILL_HANDLE);
}

type Box = { x: number; y: number; width: number; height: number };

/** fills `rect` except where `hole` is: at most four pieces around it */
function fillAround(ctx: CanvasRenderingContext2D, rect: Box, hole: Box) {
  const top = Math.max(rect.y, Math.min(hole.y, rect.y + rect.height));
  const bottom = Math.min(rect.y + rect.height, Math.max(hole.y + hole.height, rect.y));
  const left = Math.max(rect.x, Math.min(hole.x, rect.x + rect.width));
  const right = Math.min(rect.x + rect.width, Math.max(hole.x + hole.width, rect.x));
  ctx.fillRect(rect.x, rect.y, rect.width, top - rect.y);
  ctx.fillRect(rect.x, bottom, rect.width, rect.y + rect.height - bottom);
  ctx.fillRect(rect.x, top, left - rect.x, bottom - top);
  ctx.fillRect(right, top, rect.x + rect.width - right, bottom - top);
}

/** where the fill handle is: the small square at the selection's bottom right corner. */
export function fillHandleAt(layout: GridLayout, range: Range): { x: number; y: number } {
  const rect = layout.rectOf(range);
  return { x: rect.x + rect.width - FILL_HANDLE / 2 - 1, y: rect.y + rect.height - FILL_HANDLE / 2 - 1 };
}

function paintReferences(ctx: CanvasRenderingContext2D, layout: GridLayout, references: Overlay["references"]) {
  ctx.lineWidth = 2;
  for (const { range, colour } of references) {
    const rect = layout.rectOf(range);
    ctx.strokeStyle = colour;
    ctx.strokeRect(rect.x + 1, rect.y + 1, rect.width - 2, rect.height - 2);
    // excel's small square handles at the corners of a coloured reference
    ctx.fillStyle = colour;
    for (const [x, y] of [[rect.x, rect.y], [rect.x + rect.width, rect.y], [rect.x, rect.y + rect.height], [rect.x + rect.width, rect.y + rect.height]]) {
      ctx.fillRect(x - 2, y - 2, 4, 4);
    }
  }
}

function paintDashed(ctx: CanvasRenderingContext2D, layout: GridLayout, range: Range, colour: string) {
  const rect = layout.rectOf(range);
  ctx.strokeStyle = colour;
  ctx.lineWidth = 2;
  ctx.setLineDash([5, 3]);
  ctx.strokeRect(rect.x + 1, rect.y + 1, rect.width - 2, rect.height - 2);
  ctx.setLineDash([]);
}

/** the small ▾ buttons on a filter's header row; `filterButtonAt` finds them for clicks. */
function paintFilterButtons(ctx: CanvasRenderingContext2D, layout: GridLayout, filter: Range, theme: Theme) {
  for (let col = filter.start.col; col <= filter.end.col; col += 1) {
    const button = filterButtonRect(layout, filter.start.row, col);
    ctx.fillStyle = "#f3f3f3";
    ctx.strokeStyle = "#a6a6a6";
    ctx.lineWidth = 1;
    ctx.fillRect(button.x, button.y, FILTER_BUTTON, FILTER_BUTTON);
    ctx.strokeRect(button.x + 0.5, button.y + 0.5, FILTER_BUTTON - 1, FILTER_BUTTON - 1);
    ctx.fillStyle = theme.headerText;
    ctx.beginPath();
    ctx.moveTo(button.x + 4, button.y + 6);
    ctx.lineTo(button.x + 10, button.y + 6);
    ctx.lineTo(button.x + 7, button.y + 9);
    ctx.fill();
  }
}

function filterButtonRect(layout: GridLayout, row: number, col: number) {
  const right = layout.rightOf(col);
  const bottom = layout.bottomOf(row);
  return { x: right - FILTER_BUTTON - 2, y: bottom - FILTER_BUTTON - 2 };
}

/** the column whose filter button is under a canvas point, or null. */
export function filterButtonAt(layout: GridLayout, filter: Range | null, x: number, y: number): number | null {
  if (!filter) return null;
  for (let col = filter.start.col; col <= filter.end.col; col += 1) {
    const button = filterButtonRect(layout, filter.start.row, col);
    if (x >= button.x && x <= button.x + FILTER_BUTTON && y >= button.y && y <= button.y + FILTER_BUTTON) return col;
  }
  return null;
}

/** charts, pictures and shapes are kept as they are; a quiet dashed card shows where */
function paintPlaceholders(ctx: CanvasRenderingContext2D, layout: GridLayout, placeholders: Placeholder[]) {
  for (const placeholder of placeholders) {
    const rect = layout.rectOf({ start: placeholder.from, end: placeholder.to });
    if (rect.x > layout.width || rect.y > layout.height || rect.x + rect.width < 0 || rect.y + rect.height < 0) continue;
    ctx.fillStyle = "rgb(245 245 247 / 0.94)";
    ctx.fillRect(rect.x + 2, rect.y + 2, rect.width - 4, rect.height - 4);
    ctx.strokeStyle = "#9a9aa0";
    ctx.setLineDash([6, 4]);
    ctx.lineWidth = 1;
    ctx.strokeRect(rect.x + 2.5, rect.y + 2.5, rect.width - 5, rect.height - 5);
    ctx.setLineDash([]);
    ctx.fillStyle = "#3c3c43";
    ctx.font = "600 13px system-ui, 'Segoe UI', sans-serif";
    const kind = { chart: "Chart", picture: "Picture", shape: "Shape" }[placeholder.kind];
    ctx.fillText(`${kind} — kept as it is`, rect.x + 12, rect.y + 24);
    ctx.font = "12px system-ui, 'Segoe UI', sans-serif";
    ctx.fillText(placeholder.title, rect.x + 12, rect.y + 42);
  }
}

function paintFrozenLines(ctx: CanvasRenderingContext2D, layout: GridLayout) {
  ctx.fillStyle = "#b9b9be";
  if (layout.frozenRows > 0) ctx.fillRect(layout.headerWidth, layout.headerHeight + layout.frozenHeight - 1, layout.width, 1.5);
  if (layout.frozenCols > 0) ctx.fillRect(layout.headerWidth + layout.frozenWidth - 1, layout.headerHeight, 1.5, layout.height);
}

/** the column letters and row numbers, with the selected ones marked. */
export function paintHeaders(ctx: CanvasRenderingContext2D, layout: GridLayout, selection: Selection, theme: Theme) {
  ctx.fillStyle = theme.header;
  ctx.fillRect(0, 0, layout.width, layout.headerHeight);
  ctx.fillRect(0, 0, layout.headerWidth, layout.height);
  ctx.font = theme.headerFont;
  ctx.textBaseline = "middle";
  ctx.textAlign = "center";
  for (const region of layout.regions()) {
    if (region.y === layout.headerHeight + layout.frozenHeight || region.firstRow === 0) paintColumnHeaders(ctx, layout, region.firstCol, region.lastCol, selection, theme);
    if (region.x === layout.headerWidth + layout.frozenWidth || region.firstCol === 0) paintRowHeaders(ctx, layout, region.firstRow, region.lastRow, selection, theme);
  }
  ctx.textAlign = "start";
  ctx.strokeStyle = theme.headerLine;
  ctx.beginPath();
  ctx.moveTo(0, layout.headerHeight - 0.5);
  ctx.lineTo(layout.width, layout.headerHeight - 0.5);
  ctx.moveTo(layout.headerWidth - 0.5, 0);
  ctx.lineTo(layout.headerWidth - 0.5, layout.height);
  ctx.stroke();
}

function selectedCols(selection: Selection, col: number): "whole" | "part" | null {
  let found: "whole" | "part" | null = null;
  for (const range of selection.ranges) {
    if (col < range.start.col || col > range.end.col) continue;
    if (isWholeColumns(range)) return "whole";
    found = "part";
  }
  return found;
}

function selectedRows(selection: Selection, row: number): "whole" | "part" | null {
  let found: "whole" | "part" | null = null;
  for (const range of selection.ranges) {
    if (row < range.start.row || row > range.end.row) continue;
    if (isWholeRows(range)) return "whole";
    found = "part";
  }
  return found;
}

function paintColumnHeaders(ctx: CanvasRenderingContext2D, layout: GridLayout, first: number, last: number, selection: Selection, theme: Theme) {
  for (let col = first; col <= last; col += 1) {
    const x = layout.xOf(col);
    const width = layout.rightOf(col) - x;
    if (width <= 0 || x + width < layout.headerWidth) continue;
    const marked = selectedCols(selection, col);
    if (marked) {
      ctx.fillStyle = marked === "whole" ? theme.accentSoft : theme.headerSelected;
      ctx.fillRect(x, 0, width, layout.headerHeight);
    }
    ctx.fillStyle = marked ? theme.accent : theme.headerText;
    ctx.fillText(columnName(col), x + width / 2, layout.headerHeight / 2);
    ctx.fillStyle = theme.headerLine;
    ctx.fillRect(Math.round(x + width) - 1, 3, 1, layout.headerHeight - 6);
  }
}

function paintRowHeaders(ctx: CanvasRenderingContext2D, layout: GridLayout, first: number, last: number, selection: Selection, theme: Theme) {
  for (let row = first; row <= last; row += 1) {
    const y = layout.yOf(row);
    const height = layout.bottomOf(row) - y;
    if (height <= 0 || y + height < layout.headerHeight) continue;
    const marked = selectedRows(selection, row);
    if (marked) {
      ctx.fillStyle = marked === "whole" ? theme.accentSoft : theme.headerSelected;
      ctx.fillRect(0, y, layout.headerWidth, height);
    }
    ctx.fillStyle = marked ? theme.accent : theme.headerText;
    ctx.fillText(String(row + 1), layout.headerWidth / 2, y + height / 2);
    ctx.fillStyle = theme.headerLine;
    ctx.fillRect(3, Math.round(y + height) - 1, layout.headerWidth - 6, 1);
  }
}

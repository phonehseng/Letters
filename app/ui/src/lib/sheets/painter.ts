// draws the cells of the grid on its canvas: fills, grid lines, text with
// excel's overflow into empty neighbours and "###" for numbers too wide,
// borders, merged cells and note marks. the sheet is paper, white in dark
// mode too, as the writer's pages are. headers and the selection are
// drawn on top by overlays.ts.

import { BLOCK_COLS, type CellCache } from "./cellCache";
import type { GridLayout, Region } from "./gridLayout";
import type { Range, StyleLook, ViewCell } from "./sheetsApi";

export const PAPER = "#ffffff";
// excel's grid lines and default text
const GRID_LINE = "#e1e1e1";
const TEXT = "#000000";
// calibri is excel's default; carlito has the same widths where calibri is missing
const FONT_FAMILIES = "Calibri, Carlito, 'Segoe UI', Arial, sans-serif";
const DEFAULT_POINTS = 11;
const PIXELS_PER_POINT = 96 / 72;
const CELL_PADDING = 3;
const NOTE_MARK = 6;

export interface PaintSheet {
  sheet: number;
  cache: CellCache;
  styles: StyleLook[];
  merged: Range[];
  showGridLines: boolean;
  zoom: number;
}

const widths = new Map<string, number>();

/** text widths are measured once per font and text; a screen repeats many */
function measure(ctx: CanvasRenderingContext2D, font: string, text: string): number {
  const key = `${font}|${text}`;
  let width = widths.get(key);
  if (width === undefined) {
    // a cache that never empties would grow with every value ever shown
    if (widths.size > 20_000) widths.clear();
    width = ctx.measureText(text).width;
    widths.set(key, width);
  }
  return width;
}

export function fontOf(style: StyleLook | undefined, zoom: number): string {
  const points = style?.size ?? DEFAULT_POINTS;
  const pixels = Math.round(points * PIXELS_PER_POINT * zoom * 10) / 10;
  const family = style?.font ? `'${style.font.replaceAll("'", "")}', ${FONT_FAMILIES}` : FONT_FAMILIES;
  return `${style?.italic ? "italic " : ""}${style?.bold ? "bold " : ""}${pixels}px ${family}`;
}

/** paints one region of the grid: everything inside its rectangle. */
export function paintRegion(ctx: CanvasRenderingContext2D, layout: GridLayout, region: Region, sheet: PaintSheet) {
  ctx.save();
  ctx.beginPath();
  ctx.rect(region.x, region.y, region.width, region.height);
  ctx.clip();
  ctx.fillStyle = PAPER;
  ctx.fillRect(region.x, region.y, region.width, region.height);
  paintFills(ctx, layout, region, sheet);
  if (sheet.showGridLines) paintGridLines(ctx, layout, region);
  paintMerged(ctx, layout, region, sheet);
  paintTexts(ctx, layout, region, sheet);
  paintBorders(ctx, layout, region, sheet);
  ctx.restore();
}

function eachCell(region: Region, sheet: PaintSheet, visit: (cell: ViewCell) => void) {
  for (let row = region.firstRow; row <= region.lastRow; row += 1) {
    for (let col = region.firstCol; col <= region.lastCol; col += 1) {
      const cell = sheet.cache.get(sheet.sheet, row, col);
      if (cell) visit(cell);
    }
  }
}

function paintFills(ctx: CanvasRenderingContext2D, layout: GridLayout, region: Region, sheet: PaintSheet) {
  eachCell(region, sheet, (cell) => {
    if (!cell.background) return;
    const x = layout.xOf(cell.col);
    const y = layout.yOf(cell.row);
    ctx.fillStyle = cell.background;
    ctx.fillRect(x, y, layout.rightOf(cell.col) - x, layout.bottomOf(cell.row) - y);
  });
}

function paintGridLines(ctx: CanvasRenderingContext2D, layout: GridLayout, region: Region) {
  ctx.strokeStyle = GRID_LINE;
  ctx.lineWidth = 1;
  ctx.beginPath();
  const bottom = region.y + region.height;
  const right = region.x + region.width;
  for (let col = region.firstCol; col <= region.lastCol + 1; col += 1) {
    if (layout.cols.isHidden(col)) continue;
    const x = Math.round(layout.xOf(col)) - 0.5;
    ctx.moveTo(x, region.y);
    ctx.lineTo(x, bottom);
  }
  for (let row = region.firstRow; row <= region.lastRow + 1; row += 1) {
    if (layout.rows.isHidden(row)) continue;
    const y = Math.round(layout.yOf(row)) - 0.5;
    ctx.moveTo(region.x, y);
    ctx.lineTo(right, y);
  }
  ctx.stroke();
}

/** a merged range is one cell: paper over the lines inside it, then its first cell's fill */
function paintMerged(ctx: CanvasRenderingContext2D, layout: GridLayout, region: Region, sheet: PaintSheet) {
  for (const merge of sheet.merged) {
    if (!intersects(merge, region)) continue;
    const rect = layout.rectOf(merge);
    const first = sheet.cache.get(sheet.sheet, merge.start.row, merge.start.col);
    ctx.fillStyle = first?.background ?? PAPER;
    ctx.fillRect(rect.x, rect.y, rect.width - 1, rect.height - 1);
    if (first) paintText(ctx, first, rect, sheet, rect.width);
  }
}

function intersects(range: Range, region: Region): boolean {
  return range.end.row >= region.firstRow && range.start.row <= region.lastRow && range.end.col >= region.firstCol && range.start.col <= region.lastCol;
}

function paintTexts(ctx: CanvasRenderingContext2D, layout: GridLayout, region: Region, sheet: PaintSheet) {
  eachCell(region, sheet, (cell) => {
    // a merged range's first cell is drawn over the whole range by paintMerged
    if (!cell.text || cell.merge) return;
    const x = layout.xOf(cell.col);
    const y = layout.yOf(cell.row);
    const rect = { x, y, width: layout.rightOf(cell.col) - x, height: layout.bottomOf(cell.row) - y };
    if (rect.width <= 0 || rect.height <= 0) return;
    paintText(ctx, cell, rect, sheet, overflowWidth(layout, region, sheet, cell, rect.width));
    if (cell.hasNote) paintNoteMark(ctx, rect);
  });
}

/** left-aligned text runs on over empty cells to its right, as in excel */
function overflowWidth(layout: GridLayout, region: Region, sheet: PaintSheet, cell: ViewCell, own: number): number {
  const style = sheet.styles[cell.style];
  if (cell.align !== "left" || style?.wrap) return own;
  let col = cell.col + 1;
  const last = Math.min(region.lastCol, cell.col + BLOCK_COLS);
  while (col <= last && !sheet.cache.get(sheet.sheet, cell.row, col)?.text) col += 1;
  return layout.xOf(col) - layout.xOf(cell.col);
}

type Rect = { x: number; y: number; width: number; height: number };

function paintText(ctx: CanvasRenderingContext2D, cell: ViewCell, rect: Rect, sheet: PaintSheet, room: number) {
  const style = sheet.styles[cell.style];
  const font = fontOf(style, sheet.zoom);
  ctx.font = font;
  ctx.fillStyle = cell.color ?? TEXT;
  ctx.textBaseline = "alphabetic";
  const fontPixels = (style?.size ?? DEFAULT_POINTS) * PIXELS_PER_POINT * sheet.zoom;
  const padding = CELL_PADDING * sheet.zoom;
  if (style?.wrap) {
    paintWrapped(ctx, cell, rect, font, fontPixels, padding, style.vertical);
    return;
  }
  let text = cell.text;
  let width = measure(ctx, font, text);
  // a number that doesn't fit shows #### rather than a misleading part of itself
  if (cell.align === "right" && width > rect.width - padding * 2 && /\d/.test(text)) {
    const hash = measure(ctx, font, "#");
    text = "#".repeat(Math.max(1, Math.floor((rect.width - padding * 2) / hash)));
    width = measure(ctx, font, text);
  }
  const x = textX(cell.align, rect, width, padding);
  const y = baseline(style?.vertical ?? "bottom", rect, fontPixels, padding);
  ctx.save();
  ctx.beginPath();
  ctx.rect(rect.x, rect.y, Math.max(rect.width, room), rect.height);
  ctx.clip();
  ctx.fillText(text, x, y);
  if (style?.underline) ctx.fillRect(x, y + 2, width, Math.max(1, sheet.zoom));
  if (style?.strike) ctx.fillRect(x, y - fontPixels * 0.3, width, Math.max(1, sheet.zoom));
  ctx.restore();
}

function textX(align: string, rect: Rect, width: number, padding: number): number {
  if (align === "right") return rect.x + rect.width - padding - width;
  if (align === "center") return rect.x + (rect.width - width) / 2;
  return rect.x + padding;
}

function baseline(vertical: string, rect: Rect, fontPixels: number, padding: number): number {
  if (vertical === "top") return rect.y + fontPixels + 1;
  if (vertical === "center") return rect.y + rect.height / 2 + fontPixels * 0.35;
  return rect.y + rect.height - padding - 1;
}

function paintWrapped(ctx: CanvasRenderingContext2D, cell: ViewCell, rect: Rect, font: string, fontPixels: number, padding: number, vertical: string) {
  const lines = wrapLines(ctx, font, cell.text, rect.width - padding * 2);
  const lineHeight = fontPixels * 1.25;
  const blockHeight = lines.length * lineHeight;
  let y = rect.y + fontPixels + 1;
  if (vertical === "bottom") y = rect.y + rect.height - blockHeight + fontPixels - padding;
  if (vertical === "center") y = rect.y + (rect.height - blockHeight) / 2 + fontPixels;
  ctx.save();
  ctx.beginPath();
  ctx.rect(rect.x, rect.y, rect.width, rect.height);
  ctx.clip();
  for (const line of lines) {
    ctx.fillText(line, textX(cell.align, rect, measure(ctx, font, line), padding), y);
    y += lineHeight;
  }
  ctx.restore();
}

/** words laid into lines no wider than `room`, keeping the cell's own line breaks */
export function wrapLines(ctx: CanvasRenderingContext2D, font: string, text: string, room: number): string[] {
  const lines: string[] = [];
  for (const paragraph of text.split("\n")) {
    let line = "";
    for (const word of paragraph.split(" ")) {
      const candidate = line ? `${line} ${word}` : word;
      if (line && measure(ctx, font, candidate) > room) {
        lines.push(line);
        line = word;
      } else line = candidate;
    }
    lines.push(line);
  }
  return lines;
}

function paintNoteMark(ctx: CanvasRenderingContext2D, rect: Rect) {
  // excel marks a note with a small red triangle in the top right corner
  ctx.fillStyle = "#d32f2f";
  ctx.beginPath();
  ctx.moveTo(rect.x + rect.width - NOTE_MARK, rect.y);
  ctx.lineTo(rect.x + rect.width, rect.y);
  ctx.lineTo(rect.x + rect.width, rect.y + NOTE_MARK);
  ctx.fill();
}

function paintBorders(ctx: CanvasRenderingContext2D, layout: GridLayout, region: Region, sheet: PaintSheet) {
  eachCell(region, sheet, (cell) => {
    const borders = sheet.styles[cell.style]?.borders;
    if (!borders || borders.every((edge) => !edge)) return;
    const x1 = Math.round(layout.xOf(cell.col)) - 0.5;
    const y1 = Math.round(layout.yOf(cell.row)) - 0.5;
    const x2 = Math.round(layout.rightOf(cell.col)) - 0.5;
    const y2 = Math.round(layout.bottomOf(cell.row)) - 0.5;
    const edges: [number, number, number, number][] = [
      [x1, y1, x2, y1],
      [x2, y1, x2, y2],
      [x1, y2, x2, y2],
      [x1, y1, x1, y2],
    ];
    borders.forEach((edge, index) => {
      if (!edge) return;
      ctx.strokeStyle = edge.color;
      ctx.lineWidth = edge.width;
      ctx.setLineDash(edge.dashed ? [3, 2] : []);
      ctx.beginPath();
      const [ax, ay, bx, by] = edges[index];
      ctx.moveTo(ax, ay);
      ctx.lineTo(bx, by);
      ctx.stroke();
    });
    ctx.setLineDash([]);
  });
}

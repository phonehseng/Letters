// the small pieces of pointer arithmetic page tools share: counting
// clicks, telling a click from a drag, a rectangle or a line from a drag,
// and freehand strokes grouped into one drawing.
//
// everything is in points on one page, so it is the same at every zoom.

import { clamp, type Point, type Rect } from "./pagePoints";
import type { PageSize } from "../layout";

// ---- clicks ----

/** presses closer together than this, in time and space, count as one double or triple click. */
export const MULTI_CLICK_MS = 500;
export const MULTI_CLICK_DISTANCE = 4;

export interface ClickRecord {
  time: number;
  page: number;
  point: Point;
  count: number;
}

/**
 * counts a press: 1, 2 for a double click, 3 for a triple click, then
 * back to 1. pointer events don't say this reliably on their own.
 */
export function countClick(previous: ClickRecord | null, time: number, page: number, point: Point): ClickRecord {
  const close =
    previous !== null &&
    previous.page === page &&
    time - previous.time <= MULTI_CLICK_MS &&
    Math.hypot(point.x - previous.point.x, point.y - previous.point.y) <= MULTI_CLICK_DISTANCE;
  const count = close ? (previous.count % 3) + 1 : 1;
  return { time, page, point, count };
}

/** a press that hardly moved is a click, not a drag. measured in css pixels. */
export const DRAG_THRESHOLD_PX = 3;

export function movedEnough(start: Point, end: Point, scale: number): boolean {
  return Math.hypot(end.x - start.x, end.y - start.y) * scale >= DRAG_THRESHOLD_PX;
}

// ---- rectangles and lines ----

/**
 * the rectangle between where a drag started and where it is now, kept
 * on the page. with `square` the shorter side grows to match the longer,
 * for squares and circles.
 */
export function rectFromDrag(start: Point, end: Point, page: PageSize, square = false): Rect {
  const to = { x: clamp(end.x, 0, page.width), y: clamp(end.y, 0, page.height) };
  let dx = to.x - start.x;
  let dy = to.y - start.y;
  if (square) {
    const side = Math.max(Math.abs(dx), Math.abs(dy));
    dx = Math.sign(dx || 1) * side;
    dy = Math.sign(dy || 1) * side;
  }
  const x = Math.min(start.x, start.x + dx);
  const y = Math.min(start.y, start.y + dy);
  const rect = { x, y, width: Math.abs(dx), height: Math.abs(dy) };
  return keepRectOnPage(rect, page);
}

/** the same rectangle moved, then shrunk if need be, until it lies on the page. */
export function keepRectOnPage(rect: Rect, page: PageSize): Rect {
  const width = Math.min(rect.width, page.width);
  const height = Math.min(rect.height, page.height);
  return {
    x: clamp(rect.x, 0, page.width - width),
    y: clamp(rect.y, 0, page.height - height),
    width,
    height,
  };
}

/** a rectangle of a given size centred on a point, kept on the page. */
export function rectAround(centre: Point, width: number, height: number, page: PageSize): Rect {
  return keepRectOnPage({ x: centre.x - width / 2, y: centre.y - height / 2, width, height }, page);
}

/**
 * the largest rectangle with the proportions `aspect` (width over height)
 * that fits inside `rect`, sharing its corner nearest the drag's start.
 */
export function fitAspect(rect: Rect, aspect: number, anchor: Point): Rect {
  if (!(aspect > 0)) return rect;
  let width = rect.width;
  let height = width / aspect;
  if (height > rect.height) {
    height = rect.height;
    width = height * aspect;
  }
  const x = anchor.x <= rect.x + rect.width / 2 ? rect.x : rect.x + rect.width - width;
  const y = anchor.y <= rect.y + rect.height / 2 ? rect.y : rect.y + rect.height - height;
  return { x, y, width, height };
}

/**
 * the line from a drag's start to the pointer, on the page. with `snap`
 * it keeps to the nearest multiple of 45 degrees, as Shift does in
 * drawing apps.
 */
export function lineFromDrag(start: Point, end: Point, page: PageSize, snap = false): { start: Point; end: Point } {
  let to = { x: clamp(end.x, 0, page.width), y: clamp(end.y, 0, page.height) };
  if (snap) {
    const dx = to.x - start.x;
    const dy = to.y - start.y;
    const length = Math.hypot(dx, dy);
    const step = Math.PI / 4;
    const angle = Math.round(Math.atan2(dy, dx) / step) * step;
    to = {
      x: clamp(start.x + Math.cos(angle) * length, 0, page.width),
      y: clamp(start.y + Math.sin(angle) * length, 0, page.height),
    };
  }
  return { start, end: to };
}

// ---- freehand strokes ----

/** points closer than this to the last one add nothing a reader could see. */
export const STROKE_MIN_STEP = 0.5;

/** adds a point to a stroke unless it is right on top of the last one. */
export function addStrokePoint(stroke: Point[], point: Point, minStep = STROKE_MIN_STEP): Point[] {
  const last = stroke[stroke.length - 1];
  if (last && Math.hypot(point.x - last.x, point.y - last.y) < minStep) return stroke;
  return [...stroke, point];
}

/**
 * strokes started within this pause after the last one ended belong to
 * the same drawing, so a word written in several strokes is one
 * annotation and one undo step.
 */
export const STROKE_GROUP_PAUSE_MS = 1000;

/** strokes waiting to be saved as one drawing. */
export interface PendingDrawing {
  page: number;
  strokes: Point[][];
  /** when the last stroke ended */
  endedAt: number;
  /** the colour, width and opacity, so a change of style starts a new drawing */
  style: string;
}

/**
 * adds a finished stroke. it joins the waiting drawing when it is on the
 * same page, in the same style and soon enough; otherwise the waiting
 * drawing is handed back to be saved and the stroke starts a new one.
 */
export function addStroke(
  pending: PendingDrawing | null,
  page: number,
  stroke: Point[],
  now: number,
  style: string,
): { pending: PendingDrawing; finished: PendingDrawing | null } {
  const joins =
    pending !== null && pending.page === page && pending.style === style && now - pending.endedAt <= STROKE_GROUP_PAUSE_MS;
  if (joins) return { pending: { ...pending, strokes: [...pending.strokes, stroke], endedAt: now }, finished: null };
  return { pending: { page, strokes: [stroke], endedAt: now, style }, finished: pending };
}

/** whether the pause after the last stroke is over, so the drawing should be saved. */
export function drawingIsDue(pending: PendingDrawing | null, now: number): boolean {
  return pending !== null && now - pending.endedAt > STROKE_GROUP_PAUSE_MS;
}

/** a stroke with a single point (a tap) is drawn as a dot: the same point twice. */
export function strokeForSaving(stroke: Point[]): Point[] {
  return stroke.length === 1 ? [stroke[0], { x: stroke[0].x + 0.01, y: stroke[0].y + 0.01 }] : stroke;
}

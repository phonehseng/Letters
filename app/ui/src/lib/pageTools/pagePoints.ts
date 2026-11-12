// turning a pointer's position on screen into a place on a page, and back.
//
// every page tool works in points (1/72 inch) from the top left of the
// page as shown, y down, which is also what the engine takes. the screen
// works in css pixels. in between sit the zoom, the scroll position and
// where the page is laid out (one column, or pairs in two page mode).
//
// the viewer describes each page on screen as a PageFrame. a pointer's
// position is first made relative to the scroll content (whose box on
// screen moves as it scrolls), then matched to a frame.

import type { PageSize, PlacedPage } from "../layout";

export interface Point {
  x: number;
  y: number;
}

/** a rectangle in points from the page's top left, y down. */
export interface Rect {
  x: number;
  y: number;
  width: number;
  height: number;
}

/** where one page sits in the scroll content, in css pixels. */
export interface PageFrame {
  /** the page, counted from 0 */
  index: number;
  left: number;
  top: number;
  width: number;
  height: number;
  /** css pixels per point */
  scale: number;
  /** the page's size in points, as shown */
  size: PageSize;
}

/** a place on a page: which page, and where on it in points. */
export interface PagePoint {
  page: number;
  x: number;
  y: number;
}

/** the frame of a laid out page whose left edge is at `left` in the content. */
export function frameOf(placed: PlacedPage, size: PageSize, left: number): PageFrame {
  return {
    index: placed.index,
    left,
    top: placed.top,
    width: placed.width,
    height: placed.height,
    // measured from the page itself rather than the zoom, so a page drawn
    // at a rounded size still maps its edges exactly onto the paper's
    scale: size.width > 0 ? placed.width / size.width : 1,
    size,
  };
}

/**
 * a position on screen made relative to the scroll content.
 * `contentBox` is the content element's box on screen
 * (getBoundingClientRect), which already moves with the scrolling.
 */
export function contentPointOf(clientX: number, clientY: number, contentBox: { left: number; top: number }): Point {
  return { x: clientX - contentBox.left, y: clientY - contentBox.top };
}

/** how far a point is from a frame, 0 when it is on it. */
function distanceToFrame(frame: PageFrame, point: Point): number {
  const dx = Math.max(frame.left - point.x, 0, point.x - (frame.left + frame.width));
  const dy = Math.max(frame.top - point.y, 0, point.y - (frame.top + frame.height));
  return Math.hypot(dx, dy);
}

/**
 * the page under a point of the content, or with `nearest` the closest
 * page when the point is in a gap between pages (so a drag that slips
 * off the edge of a page keeps selecting on it).
 */
export function frameAt(frames: PageFrame[], point: Point, nearest = false): PageFrame | null {
  let best: PageFrame | null = null;
  let bestDistance = Infinity;
  for (const frame of frames) {
    const distance = distanceToFrame(frame, point);
    if (distance === 0) return frame;
    if (distance < bestDistance) {
      best = frame;
      bestDistance = distance;
    }
  }
  return nearest ? best : null;
}

/** a point of the content as a place on one page, kept on the page. */
export function pagePointOf(frame: PageFrame, point: Point, keepOnPage = true): PagePoint {
  let x = (point.x - frame.left) / frame.scale;
  let y = (point.y - frame.top) / frame.scale;
  if (keepOnPage) {
    x = clamp(x, 0, frame.size.width);
    y = clamp(y, 0, frame.size.height);
  }
  return { page: frame.index, x, y };
}

/**
 * the place on a page under a pointer, or null when it is not over a
 * page (and `nearest` was not asked for).
 */
export function locate(
  frames: PageFrame[],
  clientX: number,
  clientY: number,
  contentBox: { left: number; top: number },
  nearest = false,
): PagePoint | null {
  const point = contentPointOf(clientX, clientY, contentBox);
  const frame = frameAt(frames, point, nearest);
  return frame ? pagePointOf(frame, point) : null;
}

/**
 * the same place on a given page, even when the pointer has left it.
 * a drag that started on a page keeps drawing on that page.
 */
export function locateOnPage(
  frame: PageFrame,
  clientX: number,
  clientY: number,
  contentBox: { left: number; top: number },
): PagePoint {
  return pagePointOf(frame, contentPointOf(clientX, clientY, contentBox));
}

/** a place on a page as a point of the content, in css pixels. */
export function contentPointFor(frame: PageFrame, point: Point): Point {
  return { x: frame.left + point.x * frame.scale, y: frame.top + point.y * frame.scale };
}

/** a rectangle in page points as css pixels from the page's own top left corner. */
export function rectOnPage(frame: PageFrame, rect: Rect): Rect {
  return {
    x: rect.x * frame.scale,
    y: rect.y * frame.scale,
    width: rect.width * frame.scale,
    height: rect.height * frame.scale,
  };
}

/** a point is on a rectangle, with `slack` points to spare all round. */
export function rectContains(rect: Rect, point: Point, slack = 0): boolean {
  return (
    point.x >= rect.x - slack &&
    point.x <= rect.x + rect.width + slack &&
    point.y >= rect.y - slack &&
    point.y <= rect.y + rect.height + slack
  );
}

export function clamp(value: number, low: number, high: number): number {
  return Math.min(Math.max(value, low), Math.max(low, high));
}

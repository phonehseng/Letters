// the crop rectangle drawn on the page: its starting place, dragging it
// or one of its handles, and keeping it on the page.
//
// everything is in points, measured from the top left of the page as it
// is shown (after any turn, inside any crop already made), which is what
// the crop tool in core/pages expects.

import type { PageSize } from "./layout";
import { dragFrame, HANDLES as FRAME_HANDLES, type Handle } from "./pageTools/objectFrame";
import type { Rect } from "./pageTools/pagePoints";

export interface CropBox {
  left: number;
  top: number;
  width: number;
  height: number;
}

/** the smallest crop core/pages accepts, in points. */
export const SMALLEST_CROP = 3;

/** which part of the rectangle is being dragged: an edge, a corner, or all of it. */
export type CropHandle = Handle;

export const HANDLES = FRAME_HANDLES;

/**
 * where the rectangle starts: half an inch in from each edge, which
 * trims a scanner's dark border, the most common reason to crop.
 */
export function startingCrop(page: PageSize): CropBox {
  const inset = Math.min(36, page.width / 4, page.height / 4);
  return { left: inset, top: inset, width: page.width - 2 * inset, height: page.height - 2 * inset };
}

/** the same rectangle moved and shrunk until it lies on the page. */
export function keepOnPage(box: CropBox, page: PageSize): CropBox {
  const width = clamp(box.width, SMALLEST_CROP, page.width);
  const height = clamp(box.height, SMALLEST_CROP, page.height);
  return {
    left: clamp(box.left, 0, page.width - width),
    top: clamp(box.top, 0, page.height - height),
    width,
    height,
  };
}

/**
 * the rectangle after dragging `handle` by (dx, dy) points from where it
 * was when the drag started. an edge never crosses the opposite edge,
 * and nothing leaves the page.
 */
export function dragCrop(start: CropBox, handle: CropHandle, dx: number, dy: number, page: PageSize): CropBox {
  if (handle === "move") return keepOnPage({ ...start, left: start.left + dx, top: start.top + dy }, page);
  // the same frame rules every object on a page follows (pageTools/objectFrame.ts)
  return asCropBox(dragFrame(asRect(start), handle, dx, dy, { page, minSize: SMALLEST_CROP }));
}

/** the crop rectangle in the shape the page tools use, and back. */
export function asRect(box: CropBox): Rect {
  return { x: box.left, y: box.top, width: box.width, height: box.height };
}

export function asCropBox(rect: Rect): CropBox {
  return { left: rect.x, top: rect.y, width: rect.width, height: rect.height };
}

/** true when the rectangle covers the whole page, so cropping would change nothing. */
export function coversPage(box: CropBox, page: PageSize): boolean {
  const near = (a: number, b: number) => Math.abs(a - b) < 0.5;
  return near(box.left, 0) && near(box.top, 0) && near(box.width, page.width) && near(box.height, page.height);
}

/** points rounded to a tenth, for the number fields. */
export function roundPoints(value: number): number {
  return Math.round(value * 10) / 10;
}

function clamp(value: number, low: number, high: number): number {
  return Math.min(Math.max(value, low), Math.max(low, high));
}

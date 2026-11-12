// the crop rectangle drawn on the page: its starting place, dragging it
// or one of its handles, and keeping it on the page.
//
// everything is in points, measured from the top left of the page as it
// is shown (after any turn, inside any crop already made), which is what
// the crop tool in core/pages expects.

import type { PageSize } from "./layout";

export interface CropBox {
  left: number;
  top: number;
  width: number;
  height: number;
}

/** the smallest crop core/pages accepts, in points. */
export const SMALLEST_CROP = 3;

/** which part of the rectangle is being dragged: an edge, a corner, or all of it. */
export type CropHandle = "move" | "n" | "s" | "e" | "w" | "ne" | "nw" | "se" | "sw";

export const HANDLES: Exclude<CropHandle, "move">[] = ["nw", "n", "ne", "e", "se", "s", "sw", "w"];

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
  let left = start.left;
  let top = start.top;
  let right = start.left + start.width;
  let bottom = start.top + start.height;
  if (handle.includes("w")) left = clamp(left + dx, 0, right - SMALLEST_CROP);
  if (handle.includes("e")) right = clamp(right + dx, left + SMALLEST_CROP, page.width);
  if (handle.includes("n")) top = clamp(top + dy, 0, bottom - SMALLEST_CROP);
  if (handle.includes("s")) bottom = clamp(bottom + dy, top + SMALLEST_CROP, page.height);
  return { left, top, width: right - left, height: bottom - top };
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

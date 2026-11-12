// the frame around a selected object on a page (an annotation, the crop
// rectangle): moving it, its eight resize handles, and the keyboard.
//
// all in points on one page. the rules: an edge never crosses the
// opposite edge, nothing gets smaller than `minSize`, nothing leaves the
// page, and with `keepAspect` (pictures) the proportions stay the same.

import { clamp, type Rect } from "./pagePoints";
import type { PageSize } from "../layout";

/** which part of the frame is dragged: an edge, a corner, or all of it. */
export type Handle = "move" | "n" | "s" | "e" | "w" | "ne" | "nw" | "se" | "sw";

export type ResizeHandle = Exclude<Handle, "move">;

export const HANDLES: ResizeHandle[] = ["nw", "n", "ne", "e", "se", "s", "sw", "w"];
export const CORNER_HANDLES: ResizeHandle[] = ["nw", "ne", "se", "sw"];

export interface FrameRules {
  page: PageSize;
  /** the smallest width and height, in points */
  minSize: number;
  /** keep width over height as it was when the drag started */
  keepAspect?: boolean;
}

/** where each handle sits on a rectangle, as fractions of its width and height. */
export const HANDLE_SPOTS: Record<ResizeHandle, { x: number; y: number }> = {
  nw: { x: 0, y: 0 },
  n: { x: 0.5, y: 0 },
  ne: { x: 1, y: 0 },
  e: { x: 1, y: 0.5 },
  se: { x: 1, y: 1 },
  s: { x: 0.5, y: 1 },
  sw: { x: 0, y: 1 },
  w: { x: 0, y: 0.5 },
};

/** the handles a frame shows: all eight, corners only for pictures, none when it can't be resized. */
export function handlesFor(resizable: boolean, keepAspect: boolean): ResizeHandle[] {
  if (!resizable) return [];
  return keepAspect ? CORNER_HANDLES : HANDLES;
}

/** the rectangle moved by (dx, dy), kept on the page. */
export function moveFrame(start: Rect, dx: number, dy: number, page: PageSize): Rect {
  return {
    ...start,
    x: clamp(start.x + dx, 0, page.width - start.width),
    y: clamp(start.y + dy, 0, page.height - start.height),
  };
}

/**
 * the rectangle after dragging `handle` by (dx, dy) points from where it
 * was when the drag started.
 */
export function dragFrame(start: Rect, handle: Handle, dx: number, dy: number, rules: FrameRules): Rect {
  if (handle === "move") return moveFrame(start, dx, dy, rules.page);
  if (rules.keepAspect && start.width > 0 && start.height > 0) return dragKeepingAspect(start, handle, dx, dy, rules);
  const { page, minSize } = rules;
  let left = start.x;
  let top = start.y;
  let right = start.x + start.width;
  let bottom = start.y + start.height;
  if (handle.includes("w")) left = clamp(left + dx, 0, right - minSize);
  if (handle.includes("e")) right = clamp(right + dx, left + minSize, page.width);
  if (handle.includes("n")) top = clamp(top + dy, 0, bottom - minSize);
  if (handle.includes("s")) bottom = clamp(bottom + dy, top + minSize, page.height);
  return { x: left, y: top, width: right - left, height: bottom - top };
}

/**
 * resizing with the proportions kept: the corner opposite the handle
 * stays put (an edge handle keeps the middle of its opposite edge put),
 * and the size follows whichever way the pointer moved further.
 */
function dragKeepingAspect(start: Rect, handle: ResizeHandle, dx: number, dy: number, rules: FrameRules): Rect {
  const aspect = start.width / start.height;
  const grow = {
    x: handle.includes("e") ? dx : handle.includes("w") ? -dx : 0,
    y: handle.includes("s") ? dy : handle.includes("n") ? -dy : 0,
  };
  const byWidth = (start.width + grow.x) / start.width;
  const byHeight = (start.height + grow.y) / start.height;
  const usesX = handle.includes("e") || handle.includes("w");
  const usesY = handle.includes("n") || handle.includes("s");
  let factor = usesX && usesY ? (Math.abs(byWidth - 1) >= Math.abs(byHeight - 1) ? byWidth : byHeight) : usesX ? byWidth : byHeight;
  const smallest = Math.max(rules.minSize / start.width, rules.minSize / start.height);
  factor = Math.max(factor, smallest);

  // the fixed point: the opposite corner, or the middle of the opposite edge
  const fixed = {
    x: handle.includes("e") ? start.x : handle.includes("w") ? start.x + start.width : start.x + start.width / 2,
    y: handle.includes("s") ? start.y : handle.includes("n") ? start.y + start.height : start.y + start.height / 2,
  };
  const spot = HANDLE_SPOTS[handle];
  // how far the frame may grow from the fixed point before leaving the page
  const roomX = spot.x === 1 ? rules.page.width - fixed.x : spot.x === 0 ? fixed.x : 2 * Math.min(fixed.x, rules.page.width - fixed.x);
  const roomY = spot.y === 1 ? rules.page.height - fixed.y : spot.y === 0 ? fixed.y : 2 * Math.min(fixed.y, rules.page.height - fixed.y);
  factor = Math.min(factor, roomX / start.width, roomY / start.height);

  const width = start.width * factor;
  const height = width / aspect;
  const x = spot.x === 1 ? fixed.x : spot.x === 0 ? fixed.x - width : fixed.x - width / 2;
  const y = spot.y === 1 ? fixed.y : spot.y === 0 ? fixed.y - height : fixed.y - height / 2;
  return { x, y, width, height };
}

const ARROWS: Record<string, [number, number]> = {
  ArrowLeft: [-1, 0],
  ArrowRight: [1, 0],
  ArrowUp: [0, -1],
  ArrowDown: [0, 1],
};

/** the step an arrow key moves an object: 1 point, or 10 with shift. */
export function arrowStep(key: string, shift: boolean): [number, number] | null {
  const direction = ARROWS[key];
  if (!direction) return null;
  const amount = shift ? 10 : 1;
  return [direction[0] * amount, direction[1] * amount];
}

/**
 * what an arrow key does to a selected object: move it, or with alt
 * (option on a mac) make it wider or taller from its bottom right corner.
 * null for keys that aren't arrows.
 */
export function frameAfterKey(
  rect: Rect,
  key: string,
  modifiers: { shift: boolean; alt: boolean },
  rules: FrameRules & { resizable: boolean },
): Rect | null {
  const step = arrowStep(key, modifiers.shift);
  if (!step) return null;
  if (modifiers.alt) {
    if (!rules.resizable) return rect;
    return dragFrame(rect, "se", step[0], step[1], rules);
  }
  return moveFrame(rect, step[0], step[1], rules.page);
}

/** the same rectangle, unless both are the same to within a hundredth of a point. */
export function sameRect(a: Rect, b: Rect): boolean {
  const near = (p: number, q: number) => Math.abs(p - q) < 0.01;
  return near(a.x, b.x) && near(a.y, b.y) && near(a.width, b.width) && near(a.height, b.height);
}

/** the cursor that shows how a handle resizes. */
export function cursorFor(handle: Handle): string {
  if (handle === "move") return "move";
  if (handle === "n" || handle === "s") return "ns-resize";
  if (handle === "e" || handle === "w") return "ew-resize";
  return handle === "nw" || handle === "se" ? "nwse-resize" : "nesw-resize";
}

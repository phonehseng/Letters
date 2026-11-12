// the object frame's maths, kept out of the component so it can be
// tested: a resized box (size from resize.ts, and where it now sits so the
// opposite side stays put), and snap guides from page twips to the screen.

import { resize, type Handle, type ResizeOptions } from "./resize";
import type { SnapGuide } from "./snapping";

export interface Box {
  left: number;
  top: number;
  width: number;
  height: number;
}

/** the box after dragging `handle`: the opposite edge (or the centre, from the centre) stays where it was. */
export function resizedBox(start: Box, handle: Handle, dx: number, dy: number, options: ResizeOptions): Box & { snappedTo: number | null } {
  const size = resize({ width: start.width, height: start.height }, handle, dx, dy, options);
  let left = start.left;
  let top = start.top;
  if (options.fromCentre) {
    left = start.left + (start.width - size.width) / 2;
    top = start.top + (start.height - size.height) / 2;
  } else {
    if (handle.includes("w")) left = start.left + start.width - size.width;
    // side handles keep the middle of the other direction when proportions follow
    else if (!handle.includes("e")) left = start.left + (start.width - size.width) / 2;
    if (handle.includes("n")) top = start.top + start.height - size.height;
    else if (!handle.includes("s")) top = start.top + (start.height - size.height) / 2;
  }
  return { left, top, width: size.width, height: size.height, snappedTo: size.snappedTo };
}

/** a guide line on screen: from page twips to fixed screen coordinates. */
export function guideOnScreen(
  guide: SnapGuide,
  page: { edgeTop: number; widthTwips: number; heightTwips: number; marginLeftTwips: number },
  root: { left: number; top: number },
  zoom: number,
): { left: number; top: number; width: number; height: number } {
  const pageLeft = root.left - (page.marginLeftTwips / 15) * zoom;
  const pageTop = root.top + page.edgeTop * zoom;
  if (guide.vertical) return { left: pageLeft + (guide.position / 15) * zoom, top: pageTop, width: 0, height: (page.heightTwips / 15) * zoom };
  return { left: pageLeft, top: pageTop + (guide.position / 15) * zoom, width: (page.widthTwips / 15) * zoom, height: 0 };
}

/** the keyboard's step for moving and resizing: an eighth of an inch, or a quarter centimetre, in twips. */
export function keyStep(unit: "in" | "cm"): number {
  return unit === "in" ? 180 : 142;
}

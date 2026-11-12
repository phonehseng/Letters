// resizing and moving pictures: pure maths for the eight handles, locked
// proportions, resizing from the centre, snapping to useful sizes and
// places, and where a picture lands when it becomes free or goes back in
// line. the frame (ImageFrame.svelte) only previews with a css transform
// while dragging and commits once on release, so a drag is one undo step
// and nothing else on the page moves until then.

export type Handle = "n" | "ne" | "e" | "se" | "s" | "sw" | "w" | "nw";

export const HANDLES: Handle[] = ["nw", "n", "ne", "e", "se", "s", "sw", "w"];

export interface Size {
  width: number;
  height: number;
}

export interface ResizeOptions {
  /** keep the proportions (the default; Shift unlocks) */
  lockAspect: boolean;
  /** grow or shrink around the centre (Alt) */
  fromCentre: boolean;
  /** sizes to snap to (the original size, the text width...), in the same units */
  snapWidths?: number[];
  /** how close counts as "at" a snap target */
  snapDistance?: number;
  minimum?: number;
}

export interface ResizeResult extends Size {
  /** the snap target that was used, for drawing its guide */
  snappedTo: number | null;
}

/** the size after dragging `handle` by dx, dy (screen units, same as the size). */
export function resize(start: Size, handle: Handle, dx: number, dy: number, options: ResizeOptions): ResizeResult {
  const factor = options.fromCentre ? 2 : 1;
  const horizontal = handle.includes("e") ? 1 : handle.includes("w") ? -1 : 0;
  const vertical = handle.includes("s") ? 1 : handle.includes("n") ? -1 : 0;
  const minimum = options.minimum ?? 8;
  let width = Math.max(minimum, start.width + horizontal * dx * factor);
  let height = Math.max(minimum, start.height + vertical * dy * factor);
  const ratio = start.width / Math.max(1, start.height);
  if (options.lockAspect) {
    // a side handle drives its own dimension; a corner follows the larger change
    if (horizontal === 0) width = height * ratio;
    else if (vertical === 0) height = width / ratio;
    else if (Math.abs(width - start.width) >= Math.abs(height - start.height) * ratio) height = width / ratio;
    else width = height * ratio;
  }
  let snappedTo: number | null = null;
  const distance = options.snapDistance ?? 6;
  for (const target of options.snapWidths ?? []) {
    if (Math.abs(width - target) <= distance) {
      snappedTo = target;
      if (options.lockAspect) height = target / ratio;
      width = target;
      break;
    }
  }
  return { width: Math.round(width), height: Math.round(height), snappedTo };
}

/** the transform origin for previewing a drag of `handle` (the opposite corner stays put). */
export function previewOrigin(handle: Handle, fromCentre: boolean): string {
  if (fromCentre) return "50% 50%";
  const x = handle.includes("e") ? "0%" : handle.includes("w") ? "100%" : "50%";
  const y = handle.includes("s") ? "0%" : handle.includes("n") ? "100%" : "50%";
  return `${x} ${y}`;
}

/** a size that fits `limit` wide, keeping proportions (Fit to Page Width). */
export function fitWidth(size: Size, limit: number): Size {
  if (size.width <= limit) return size;
  return { width: Math.round(limit), height: Math.round((size.height * limit) / size.width) };
}

/** the keyboard: Alt and an arrow makes a picture a step bigger or smaller, keeping proportions. */
export function nudgeSize(size: Size, direction: 1 | -1, step: number): Size {
  const width = Math.max(step, size.width + direction * step);
  return { width, height: Math.round((width * size.height) / Math.max(1, size.width)) };
}

export interface Guide {
  axis: "x" | "y";
  at: number;
}

/**
 * where a free picture lands after a move, snapped to the left margin, the
 * right margin and the centre of the text column (x), with the guide drawn.
 */
export function snapPosition(left: number, width: number, columnWidth: number, distance = 6): { left: number; guide: Guide | null } {
  const targets = [
    { left: 0, guide: 0 },
    { left: columnWidth - width, guide: columnWidth },
    { left: (columnWidth - width) / 2, guide: columnWidth / 2 },
  ];
  for (const target of targets) {
    if (Math.abs(left - target.left) <= distance) return { left: Math.round(target.left), guide: { axis: "x", at: target.guide } };
  }
  return { left: Math.round(left), guide: null };
}

/**
 * a free picture's offsets (from its paragraph's left and top) that keep it
 * exactly where it is on screen, in twips. `scale` is screen pixels per
 * twip (zoom / 15).
 */
export function freeOffsets(image: { left: number; top: number }, paragraph: { left: number; top: number }, scale: number): { horizontal: number; vertical: number } {
  return {
    horizontal: Math.round((image.left - paragraph.left) / scale),
    vertical: Math.round((image.top - paragraph.top) / scale),
  };
}

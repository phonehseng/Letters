// cropping and rotating pictures: pure maths for the frame.
//
// word stores a crop as how much is cut from each side, in thousandths of
// a percent of the whole picture (50000 is half), and the picture's size
// as the size of the part that shows. so cropping a side makes the shown
// picture smaller while the rest of it keeps its scale, and uncropping
// gives the hidden part back at the same scale.

import type { Handle } from "./resize";

export interface Crop {
  left: number;
  top: number;
  right: number;
  bottom: number;
}

export const FULL = 100000;
const NO_CROP: Crop = { left: 0, top: 0, right: 0, bottom: 0 };

export function cropOf(data: Record<string, unknown>): Crop {
  return { ...NO_CROP, ...((data.crop as Partial<Crop> | undefined) ?? {}) };
}

/** the whole picture's size, from the size that shows and the crop. */
export function uncroppedSize(size: { width: number; height: number }, crop: Crop): { width: number; height: number } {
  const keptX = Math.max(1, FULL - crop.left - crop.right) / FULL;
  const keptY = Math.max(1, FULL - crop.top - crop.bottom) / FULL;
  return { width: size.width / keptX, height: size.height / keptY };
}

/** where the whole picture sits inside the shown part (for drawing: offsets are negative or zero). */
export function cropPlacement(size: { width: number; height: number }, crop: Crop): { left: number; top: number; width: number; height: number } {
  const whole = uncroppedSize(size, crop);
  return { left: (-whole.width * crop.left) / FULL, top: (-whole.height * crop.top) / FULL, width: whole.width, height: whole.height };
}

export interface CropResult {
  crop: Crop;
  width: number;
  height: number;
  /** how far the shown part's top-left corner moved (a left or top crop moves it) */
  shiftX: number;
  shiftY: number;
}

/**
 * dragging a crop handle by dx, dy (in the same units as the size): the
 * side under the handle moves, the picture under it stays put. a crop
 * never goes below nothing on a side, and at least `minimum` stays shown.
 */
export function cropDrag(start: { width: number; height: number }, crop: Crop, handle: Handle, dx: number, dy: number, minimum = 8): CropResult {
  const whole = uncroppedSize(start, crop);
  const next = { ...crop };
  let { width, height } = start;
  let shiftX = 0;
  let shiftY = 0;
  const perX = FULL / whole.width;
  const perY = FULL / whole.height;
  if (handle.includes("e")) {
    const right = clamp(crop.right - dx * perX, 0, FULL - crop.left - minimum * perX);
    width = whole.width * ((FULL - crop.left - right) / FULL);
    next.right = Math.round(right);
  }
  if (handle.includes("w")) {
    const left = clamp(crop.left + dx * perX, 0, FULL - crop.right - minimum * perX);
    width = whole.width * ((FULL - left - crop.right) / FULL);
    shiftX = ((left - crop.left) / FULL) * whole.width;
    next.left = Math.round(left);
  }
  if (handle.includes("s")) {
    const bottom = clamp(crop.bottom - dy * perY, 0, FULL - crop.top - minimum * perY);
    height = whole.height * ((FULL - crop.top - bottom) / FULL);
    next.bottom = Math.round(bottom);
  }
  if (handle.includes("n")) {
    const top = clamp(crop.top + dy * perY, 0, FULL - crop.bottom - minimum * perY);
    height = whole.height * ((FULL - top - crop.bottom) / FULL);
    shiftY = ((top - crop.top) / FULL) * whole.height;
    next.top = Math.round(top);
  }
  return { crop: next, width, height, shiftX, shiftY };
}

function clamp(value: number, low: number, high: number): number {
  return Math.min(Math.max(value, low), Math.max(low, high));
}

/** no crop at all: the model leaves the field out then. */
export function isUncropped(crop: Crop): boolean {
  return !crop.left && !crop.top && !crop.right && !crop.bottom;
}

// --- rotation

/** the angle of the pointer around the centre, clockwise from straight up, 0 to 360. */
export function angleFromPointer(centre: { x: number; y: number }, pointer: { x: number; y: number }): number {
  const radians = Math.atan2(pointer.x - centre.x, centre.y - pointer.y);
  return normalAngle((radians * 180) / Math.PI);
}

/** angles snap to 15 degree steps unless Shift frees them. */
export function snapAngle(angle: number, free: boolean, step = 15): number {
  const value = free ? Math.round(angle * 10) / 10 : Math.round(angle / step) * step;
  return normalAngle(value);
}

export function normalAngle(angle: number): number {
  // rounded to a millionth, so 52.3 stays 52.3 after the wrap-around sums
  const value = Math.round((((angle % 360) + 360) % 360) * 1e6) / 1e6;
  return value === 360 ? 0 : value;
}

/** the box a rotated rectangle covers (word wraps text around this box). */
export function rotatedBounds(rect: { left: number; top: number; width: number; height: number }, degrees: number): { left: number; top: number; width: number; height: number } {
  const radians = (degrees * Math.PI) / 180;
  const cos = Math.abs(Math.cos(radians));
  const sin = Math.abs(Math.sin(radians));
  const width = rect.width * cos + rect.height * sin;
  const height = rect.width * sin + rect.height * cos;
  const cx = rect.left + rect.width / 2;
  const cy = rect.top + rect.height / 2;
  return { left: cx - width / 2, top: cy - height / 2, width, height };
}

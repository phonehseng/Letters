// the canvas's maths, kept out of the components so it can be tested:
// fitting the slide in the window, which tiles a slide picture needs at a
// size, finding the object under the pointer (objects can be turned),
// turning with a handle (15° steps unless Shift is held), and snapping a
// moved box to the slide's edges, its centre and the other objects
// (the writer's snapping, lib/writer/snapping.ts, does the work).
//
// everything is in slide points unless a name says pixels.

import { resizedBox } from "../writer/frameMaths";
import type { Handle } from "../writer/resize";
import { snap, type PageBox, type SnapGuide } from "../writer/snapping";

/** must match TILE_SIZE in core/pdf/src/geometry.rs */
export const TILE_SIZE = 512;

/** turning snaps to these steps, as powerpoint and keynote do with Shift up */
export const ROTATION_STEP = 15;

/** how far snapping reaches, in screen pixels (the writer uses the same) */
export const SNAP_PIXELS = 6;

export interface Box {
  x: number;
  y: number;
  width: number;
  height: number;
}

export interface Turned extends Box {
  /** clockwise degrees about the centre */
  rotation: number;
}

/** pixels per point that fit the slide inside the space, never above `largest`. */
export function fitScale(slide: { width: number; height: number }, space: { width: number; height: number }, largest = 4): number {
  if (space.width <= 0 || space.height <= 0) return 0.1;
  return Math.min(space.width / slide.width, space.height / slide.height, largest);
}

export interface Tile {
  column: number;
  row: number;
  left: number;
  top: number;
  width: number;
  height: number;
}

/**
 * the tiles of a slide drawn `scale` pixels per point, at device pixel
 * ratio `ratio`: the renderer draws in device pixels, and css shows them
 * at `1 / ratio` the size so the slide stays sharp.
 */
export function tilesFor(slide: { width: number; height: number }, scale: number, ratio: number): { scaleMilli: number; tiles: Tile[] } {
  const scaleMilli = Math.max(1, Math.round(scale * ratio * 1000));
  const pixelsWide = Math.ceil((slide.width * scaleMilli) / 1000);
  const pixelsHigh = Math.ceil((slide.height * scaleMilli) / 1000);
  const tiles: Tile[] = [];
  for (let row = 0; row * TILE_SIZE < pixelsHigh; row++) {
    for (let column = 0; column * TILE_SIZE < pixelsWide; column++) {
      tiles.push({
        column,
        row,
        left: (column * TILE_SIZE) / ratio,
        top: (row * TILE_SIZE) / ratio,
        width: Math.min(TILE_SIZE, pixelsWide - column * TILE_SIZE) / ratio,
        height: Math.min(TILE_SIZE, pixelsHigh - row * TILE_SIZE) / ratio,
      });
    }
  }
  return { scaleMilli, tiles };
}

/** a point turned `degrees` clockwise about a centre (y points down). */
export function turnPoint(point: { x: number; y: number }, centre: { x: number; y: number }, degrees: number): { x: number; y: number } {
  const radians = (degrees * Math.PI) / 180;
  const dx = point.x - centre.x;
  const dy = point.y - centre.y;
  return { x: centre.x + dx * Math.cos(radians) - dy * Math.sin(radians), y: centre.y + dx * Math.sin(radians) + dy * Math.cos(radians) };
}

/** whether a slide point lies inside a turned box. */
export function contains(box: Turned, point: { x: number; y: number }): boolean {
  const centre = { x: box.x + box.width / 2, y: box.y + box.height / 2 };
  const local = turnPoint(point, centre, -box.rotation);
  // a line has no height: give thin objects a few points to be grabbed by
  const slack = 3;
  return local.x >= box.x - slack && local.x <= box.x + box.width + slack && local.y >= box.y - slack && local.y <= box.y + box.height + slack;
}

/** the frontmost object at a point (objects come back to front). */
export function objectAt<T extends Turned>(objects: T[], point: { x: number; y: number }): T | null {
  for (let index = objects.length - 1; index >= 0; index--) {
    if (contains(objects[index], point)) return objects[index];
  }
  return null;
}

/** the angle of a turning handle dragged to `pointer`, snapped unless `free`. */
export function rotationFor(box: Box, pointer: { x: number; y: number }, free: boolean): number {
  const centre = { x: box.x + box.width / 2, y: box.y + box.height / 2 };
  // the handle sits straight above the centre at 0°
  const degrees = (Math.atan2(pointer.x - centre.x, centre.y - pointer.y) * 180) / Math.PI;
  const turned = (degrees + 360) % 360;
  if (free) return Math.round(turned * 10) / 10;
  return (Math.round(turned / ROTATION_STEP) * ROTATION_STEP) % 360;
}

/** a box moved by dx, dy and snapped to the slide and the other objects. */
export function snappedMove(
  slide: { width: number; height: number },
  start: Box,
  dx: number,
  dy: number,
  others: Box[],
  pixelsPerPoint: number,
): { box: Box; guides: SnapGuide[] } {
  const proposed: PageBox = { x: start.x + dx, y: start.y + dy, width: start.width, height: start.height };
  const page = { width: slide.width, height: slide.height, margins: { top: 0, right: 0, bottom: 0, left: 0 } };
  const threshold = SNAP_PIXELS / Math.max(pixelsPerPoint, 0.05);
  // the writer's snap truncates halves to whole units; points are fine enough for that
  const { rect, guides } = snap(page, others, proposed, threshold);
  return { box: rect, guides };
}

/**
 * a turned box resized by dragging `handle` by dx, dy (slide points): the
 * drag is read in the box's own turned axes, and the box is shifted so the
 * opposite handle stays exactly where it was on the slide, as powerpoint
 * keeps it. `lockAspect` keeps the proportions (Shift, or always for pictures).
 */
export function resizeTurned(start: Turned, handle: Handle, dx: number, dy: number, lockAspect: boolean): Box {
  const local = turnPoint({ x: dx, y: dy }, { x: 0, y: 0 }, -start.rotation);
  const frame = { left: start.x, top: start.y, width: start.width, height: start.height };
  const sized = resizedBox(frame, handle, local.x, local.y, { lockAspect, fromCentre: false, minimum: 1 });
  const anchor = oppositeAnchor(handle);
  const before = turnPoint(
    { x: start.x + anchor.x * start.width, y: start.y + anchor.y * start.height },
    { x: start.x + start.width / 2, y: start.y + start.height / 2 },
    start.rotation,
  );
  const after = turnPoint(
    { x: sized.left + anchor.x * sized.width, y: sized.top + anchor.y * sized.height },
    { x: sized.left + sized.width / 2, y: sized.top + sized.height / 2 },
    start.rotation,
  );
  return { x: sized.left + before.x - after.x, y: sized.top + before.y - after.y, width: sized.width, height: sized.height };
}

/** the share of the box (0 to 1 each way) the handle opposite `handle` sits at. */
function oppositeAnchor(handle: Handle): { x: number; y: number } {
  const x = handle.includes("e") ? 0 : handle.includes("w") ? 1 : 0.5;
  const y = handle.includes("s") ? 0 : handle.includes("n") ? 1 : 0.5;
  return { x, y };
}

/** the box of several boxes, for moving a selection together. */
export function boundsOf(boxes: Box[]): Box {
  const left = Math.min(...boxes.map((box) => box.x));
  const top = Math.min(...boxes.map((box) => box.y));
  const right = Math.max(...boxes.map((box) => box.x + box.width));
  const bottom = Math.max(...boxes.map((box) => box.y + box.height));
  return { x: left, y: top, width: right - left, height: bottom - top };
}

/** where a new object goes: centred on the slide, `share` of its width. */
export function centredBox(slide: { width: number; height: number }, aspect: number, share = 0.3): Box {
  const width = slide.width * share;
  const height = width / aspect;
  return { x: (slide.width - width) / 2, y: (slide.height - height) / 2, width, height };
}

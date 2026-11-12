// the geometry of selecting and reshaping vector shapes on a page: which
// shapes a click selects (a logo is one object), the box around them, the
// target a thin line needs to be clickable, and where a rectangle's other
// corners go when one corner is dragged.

import type { Point, Rect } from "../pageTools/pagePoints";
import type { PageObjects, PathObject, PathShape } from "./editApi";

/** a hairline is still this wide to click (points). */
export const MIN_TARGET = 6;

/** the box around several rectangles. */
export function unionRect(rects: Rect[]): Rect {
  const left = Math.min(...rects.map((rect) => rect.x));
  const top = Math.min(...rects.map((rect) => rect.y));
  const right = Math.max(...rects.map((rect) => rect.x + rect.width));
  const bottom = Math.max(...rects.map((rect) => rect.y + rect.height));
  return { x: left, y: top, width: right - left, height: bottom - top };
}

/** a shape's click target: its box, grown to at least MIN_TARGET each way. */
export function targetRect(rect: Rect): Rect {
  const width = Math.max(rect.width, MIN_TARGET);
  const height = Math.max(rect.height, MIN_TARGET);
  return { x: rect.x - (width - rect.width) / 2, y: rect.y - (height - rect.height) / 2, width, height };
}

/** the shapes a click on `pathId` selects: its suggested group (a logo moves as one), else itself. */
export function selectionFor(objects: PageObjects, pathId: string): string[] {
  const group = (objects.pathGroups ?? []).find((each) => each.pathIds.includes(pathId));
  return group ? [...group.pathIds] : [pathId];
}

/** the shapes of `ids` still on the page. */
export function shapesOf(objects: PageObjects | undefined, ids: string[]): PathObject[] {
  const paths = objects?.paths ?? [];
  return ids.map((id) => paths.find((path) => path.id === id)).filter((path): path is PathObject => path !== undefined);
}

/**
 * a rectangle's corners (top left, top right, bottom left, bottom right)
 * after corner `index` is dragged to `to`: the two corners beside it move
 * with it so it stays a rectangle. a line's end simply moves.
 */
export function movedPoints(shape: PathShape, points: Point[], index: number, to: Point): Point[] {
  const moved = points.map((point) => ({ ...point }));
  moved[index] = { ...to };
  if (shape !== "rectangle" || points.length !== 4) return moved;
  // the corner sharing x is the one above or below; sharing y, beside it
  const sameColumn = [2, 3, 0, 1][index];
  const sameRow = [1, 0, 3, 2][index];
  moved[sameColumn] = { x: to.x, y: moved[sameColumn].y };
  moved[sameRow] = { x: moved[sameRow].x, y: to.y };
  return moved;
}

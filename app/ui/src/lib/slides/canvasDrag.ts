// what a drag on the slide canvas means, step by step, kept out of the
// component so it can be tested: moving the selection (snapped, and only
// once the pointer has really moved, so a click never shifts anything),
// resizing one object by a handle, and turning it by the round handle.
// while dragging only the preview changes; the release makes one
// operation, so a drag is one undo step.

import type { SnapGuide } from "../writer/snapping";
import type { Handle } from "../writer/resize";
import { boundsOf, resizeTurned, rotationFor, snappedMove, type Box, type Turned } from "./geometry";
import { moveBy, resizeTo, rotateTo, type Operation } from "./operations";

/** a press only becomes a drag past this many screen pixels (a steady hand still wobbles) */
export const DRAG_START_PIXELS = 3;

/** the arrows nudge a point, ten with Shift (keynote's steps; powerpoint's are similar) */
export const NUDGE_POINTS = 1;
export const BIG_NUDGE_POINTS = 10;

export interface Point {
  x: number;
  y: number;
}

export interface MoveDrag {
  kind: "move";
  ids: number[];
  from: Point;
  bounds: Box;
  others: Box[];
  started: boolean;
  dx: number;
  dy: number;
  guides: SnapGuide[];
}

export interface ResizeDrag {
  kind: "resize";
  id: number;
  handle: Handle;
  from: Point;
  start: Turned;
  box: Box;
  /** pictures keep their proportions unless Shift is held, as everywhere */
  keepsProportions: boolean;
}

export interface TurnDrag {
  kind: "turn";
  id: number;
  start: Turned;
  rotation: number;
}

export type Drag = MoveDrag | ResizeDrag | TurnDrag;

export function startMove(selected: Turned[], all: (Turned & { id: number })[], from: Point): MoveDrag {
  const ids = new Set(selected.map((box) => (box as Turned & { id: number }).id));
  return {
    kind: "move",
    ids: [...ids],
    from,
    bounds: boundsOf(selected),
    others: all.filter((box) => !ids.has(box.id)),
    started: false,
    dx: 0,
    dy: 0,
    guides: [],
  };
}

/** the drag after the pointer reached `at`; `scale` is screen pixels per point. */
export function updateMove(drag: MoveDrag, at: Point, slide: { width: number; height: number }, scale: number, free: boolean): MoveDrag {
  const dx = at.x - drag.from.x;
  const dy = at.y - drag.from.y;
  const started = drag.started || Math.hypot(dx, dy) * scale > DRAG_START_PIXELS;
  if (!started) return drag;
  // Alt moves freely, without snapping (powerpoint holds Alt for the same)
  if (free) return { ...drag, started, dx, dy, guides: [] };
  const snapped = snappedMove(slide, drag.bounds, dx, dy, drag.others, scale);
  return { ...drag, started, dx: snapped.box.x - drag.bounds.x, dy: snapped.box.y - drag.bounds.y, guides: snapped.guides };
}

export function startResize(id: number, handle: Handle, start: Turned, from: Point, keepsProportions: boolean): ResizeDrag {
  return { kind: "resize", id, handle, from, start, box: { ...start }, keepsProportions };
}

export function updateResize(drag: ResizeDrag, at: Point, shift: boolean): ResizeDrag {
  const lock = drag.keepsProportions !== shift;
  return { ...drag, box: resizeTurned(drag.start, drag.handle, at.x - drag.from.x, at.y - drag.from.y, lock) };
}

export function startTurn(id: number, start: Turned): TurnDrag {
  return { kind: "turn", id, start, rotation: start.rotation };
}

export function updateTurn(drag: TurnDrag, at: Point, free: boolean): TurnDrag {
  return { ...drag, rotation: rotationFor(drag.start, at, free) };
}

/** the one operation a finished drag makes, or null when nothing changed. */
export function dragOperation(drag: Drag, slideId: string): Operation | null {
  switch (drag.kind) {
    case "move":
      if (!drag.started || (drag.dx === 0 && drag.dy === 0)) return null;
      return moveBy(slideId, drag.ids, drag.dx, drag.dy);
    case "resize": {
      const same = drag.box.x === drag.start.x && drag.box.y === drag.start.y && drag.box.width === drag.start.width && drag.box.height === drag.start.height;
      return same ? null : resizeTo(slideId, drag.id, drag.box);
    }
    case "turn":
      return drag.rotation === drag.start.rotation ? null : rotateTo(slideId, drag.id, drag.rotation);
  }
}

/** the arrows: how far a nudge moves, or null for other keys. */
export function nudgeFor(key: string, shift: boolean): Point | null {
  const step = shift ? BIG_NUDGE_POINTS : NUDGE_POINTS;
  if (key === "ArrowLeft") return { x: -step, y: 0 };
  if (key === "ArrowRight") return { x: step, y: 0 };
  if (key === "ArrowUp") return { x: 0, y: -step };
  if (key === "ArrowDown") return { x: 0, y: step };
  return null;
}

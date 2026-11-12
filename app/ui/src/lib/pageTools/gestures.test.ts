import { describe, expect, it } from "vitest";
import {
  addStroke,
  addStrokePoint,
  countClick,
  drawingIsDue,
  fitAspect,
  keepRectOnPage,
  lineFromDrag,
  movedEnough,
  rectAround,
  rectFromDrag,
  STROKE_GROUP_PAUSE_MS,
  strokeForSaving,
} from "./gestures";

const letter = { width: 612, height: 792 };

describe("counting clicks", () => {
  it("counts quick presses in one place as double and triple clicks", () => {
    const first = countClick(null, 0, 0, { x: 10, y: 10 });
    const second = countClick(first, 200, 0, { x: 11, y: 10 });
    const third = countClick(second, 400, 0, { x: 11, y: 11 });
    expect([first.count, second.count, third.count]).toEqual([1, 2, 3]);
    // a fourth starts over
    expect(countClick(third, 600, 0, { x: 11, y: 11 }).count).toBe(1);
  });

  it("starts over when too slow, too far, or on another page", () => {
    const first = countClick(null, 0, 0, { x: 10, y: 10 });
    expect(countClick(first, 900, 0, { x: 10, y: 10 }).count).toBe(1);
    expect(countClick(first, 100, 0, { x: 30, y: 10 }).count).toBe(1);
    expect(countClick(first, 100, 1, { x: 10, y: 10 }).count).toBe(1);
  });

  it("tells a click from a drag in screen pixels, whatever the zoom", () => {
    expect(movedEnough({ x: 0, y: 0 }, { x: 1, y: 0 }, 1)).toBe(false);
    expect(movedEnough({ x: 0, y: 0 }, { x: 1, y: 0 }, 4)).toBe(true);
  });
});

describe("rectangles from a drag", () => {
  it("works in any direction and stays on the page", () => {
    expect(rectFromDrag({ x: 100, y: 100 }, { x: 50, y: 160 }, letter)).toEqual({ x: 50, y: 100, width: 50, height: 60 });
    expect(rectFromDrag({ x: 600, y: 780 }, { x: 900, y: 900 }, letter)).toEqual({ x: 600, y: 780, width: 12, height: 12 });
  });

  it("makes a square with shift, towards the pointer", () => {
    expect(rectFromDrag({ x: 100, y: 100 }, { x: 140, y: 120 }, letter, true)).toEqual({ x: 100, y: 100, width: 40, height: 40 });
    expect(rectFromDrag({ x: 100, y: 100 }, { x: 60, y: 90 }, letter, true)).toEqual({ x: 60, y: 60, width: 40, height: 40 });
  });

  it("places a rectangle around a point without leaving the page", () => {
    expect(rectAround({ x: 300, y: 400 }, 100, 50, letter)).toEqual({ x: 250, y: 375, width: 100, height: 50 });
    expect(rectAround({ x: 5, y: 5 }, 100, 50, letter)).toEqual({ x: 0, y: 0, width: 100, height: 50 });
    expect(keepRectOnPage({ x: -10, y: 0, width: 1000, height: 10 }, letter)).toEqual({ x: 0, y: 0, width: 612, height: 10 });
  });

  it("keeps a picture's proportions inside the dragged rectangle", () => {
    const dragged = { x: 100, y: 100, width: 200, height: 200 };
    expect(fitAspect(dragged, 2, { x: 100, y: 100 })).toEqual({ x: 100, y: 100, width: 200, height: 100 });
    // dragged up and left: the shape hangs from the bottom right corner
    expect(fitAspect(dragged, 0.5, { x: 300, y: 300 })).toEqual({ x: 200, y: 100, width: 100, height: 200 });
  });
});

describe("lines from a drag", () => {
  it("follows the pointer, or snaps to 45 degree steps with shift", () => {
    expect(lineFromDrag({ x: 100, y: 100 }, { x: 200, y: 110 }, letter).end).toEqual({ x: 200, y: 110 });
    const flat = lineFromDrag({ x: 100, y: 100 }, { x: 200, y: 110 }, letter, true).end;
    expect(flat.y).toBeCloseTo(100);
    const diagonal = lineFromDrag({ x: 100, y: 100 }, { x: 200, y: 190 }, letter, true).end;
    expect(diagonal.x - 100).toBeCloseTo(diagonal.y - 100);
  });
});

describe("freehand strokes", () => {
  it("skips points that would add nothing", () => {
    let stroke = addStrokePoint([], { x: 0, y: 0 });
    stroke = addStrokePoint(stroke, { x: 0.1, y: 0.1 });
    stroke = addStrokePoint(stroke, { x: 3, y: 0 });
    expect(stroke).toEqual([
      { x: 0, y: 0 },
      { x: 3, y: 0 },
    ]);
  });

  it("joins strokes made within a pause into one drawing", () => {
    const first = addStroke(null, 0, [{ x: 0, y: 0 }], 1000, "black");
    expect(first.finished).toBeNull();
    const second = addStroke(first.pending, 0, [{ x: 5, y: 5 }], 1000 + STROKE_GROUP_PAUSE_MS - 1, "black");
    expect(second.finished).toBeNull();
    expect(second.pending.strokes).toHaveLength(2);
  });

  it("starts a new drawing after a pause, on another page, or in another style", () => {
    const first = addStroke(null, 0, [{ x: 0, y: 0 }], 0, "black").pending;
    const late = addStroke(first, 0, [{ x: 1, y: 1 }], STROKE_GROUP_PAUSE_MS + 1, "black");
    expect(late.finished).toBe(first);
    expect(late.pending.strokes).toHaveLength(1);
    expect(addStroke(first, 1, [{ x: 1, y: 1 }], 10, "black").finished).toBe(first);
    expect(addStroke(first, 0, [{ x: 1, y: 1 }], 10, "red").finished).toBe(first);
  });

  it("is due for saving once the pause is over", () => {
    const pending = addStroke(null, 0, [{ x: 0, y: 0 }], 0, "black").pending;
    expect(drawingIsDue(pending, STROKE_GROUP_PAUSE_MS)).toBe(false);
    expect(drawingIsDue(pending, STROKE_GROUP_PAUSE_MS + 1)).toBe(true);
    expect(drawingIsDue(null, 99999)).toBe(false);
  });

  it("turns a tap into a dot the engine can draw", () => {
    expect(strokeForSaving([{ x: 1, y: 1 }])).toHaveLength(2);
    expect(strokeForSaving([{ x: 1, y: 1 }, { x: 2, y: 2 }])).toHaveLength(2);
  });
});

// what a drag on the slide means: a click moves nothing, a move snaps,
// resizing keeps proportions for pictures, turning makes one operation.

import { describe, expect, it } from "vitest";
import { dragOperation, nudgeFor, startMove, startResize, startTurn, updateMove, updateResize, updateTurn } from "./canvasDrag";

const SLIDE = { width: 960, height: 540 };
const shape = { id: 4, x: 100, y: 100, width: 200, height: 100, rotation: 0 };
const other = { id: 5, x: 600, y: 400, width: 100, height: 100, rotation: 0 };

describe("a drag on the slide canvas", () => {
  it("a click without moving changes nothing", () => {
    let drag = startMove([shape], [shape, other], { x: 150, y: 150 });
    drag = updateMove(drag, { x: 150.5, y: 150.5 }, SLIDE, 1, false);
    expect(drag.started).toBe(false);
    expect(dragOperation(drag, "s")).toBeNull();
  });

  it("moving makes one move operation in emu, snapped to the slide's centre", () => {
    let drag = startMove([shape], [shape, other], { x: 150, y: 150 });
    drag = updateMove(drag, { x: 427, y: 177 }, SLIDE, 1, false);
    expect(drag.dx).toBe(280);
    expect(drag.guides.length).toBeGreaterThan(0);
    expect(dragOperation(drag, "s")).toEqual({ op: "move", slide_id: "s", object_ids: [4], dx: 280 * 12700, dy: 27 * 12700 });
  });

  it("Alt moves without snapping", () => {
    let drag = startMove([shape], [shape], { x: 150, y: 150 });
    drag = updateMove(drag, { x: 427, y: 150 }, SLIDE, 1, true);
    expect(drag.dx).toBe(277);
  });

  it("resizing pictures keeps proportions unless Shift is held", () => {
    let drag = startResize(4, "e", shape, { x: 300, y: 150 }, true);
    drag = updateResize(drag, { x: 400, y: 150 }, false);
    expect(drag.box.width / drag.box.height).toBeCloseTo(2);
    drag = updateResize(drag, { x: 400, y: 150 }, true);
    expect(drag.box.height).toBe(100);
    expect(dragOperation(drag, "s")).toMatchObject({ op: "resize", object_id: 4, width: 300 * 12700 });
  });

  it("turning makes one rotate operation", () => {
    let drag = startTurn(4, shape);
    drag = updateTurn(drag, { x: 400, y: 150 }, false);
    expect(drag.rotation).toBe(90);
    expect(dragOperation(drag, "s")).toEqual({ op: "rotate", slide_id: "s", object_id: 4, rotation: 90 * 60000 });
  });

  it("the arrows nudge a point, ten with Shift", () => {
    expect(nudgeFor("ArrowLeft", false)).toEqual({ x: -1, y: 0 });
    expect(nudgeFor("ArrowDown", true)).toEqual({ x: 0, y: 10 });
    expect(nudgeFor("a", false)).toBeNull();
  });
});

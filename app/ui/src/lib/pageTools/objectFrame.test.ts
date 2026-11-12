import { describe, expect, it } from "vitest";
import {
  arrowStep,
  cursorFor,
  dragFrame,
  frameAfterKey,
  handlesFor,
  HANDLES,
  moveFrame,
  sameRect,
} from "./objectFrame";

const letter = { width: 612, height: 792 };
const rules = { page: letter, minSize: 4 };
const box = { x: 100, y: 100, width: 200, height: 100 };

describe("moving and resizing a frame", () => {
  it("moves as a whole without leaving the page", () => {
    expect(moveFrame(box, 10, -20, letter)).toEqual({ x: 110, y: 80, width: 200, height: 100 });
    expect(moveFrame(box, 1000, 1000, letter)).toEqual({ x: 412, y: 692, width: 200, height: 100 });
    expect(dragFrame(box, "move", -500, 0, rules).x).toBe(0);
  });

  it("drags each edge and corner, never past the opposite side", () => {
    expect(dragFrame(box, "e", 50, 0, rules)).toEqual({ x: 100, y: 100, width: 250, height: 100 });
    expect(dragFrame(box, "nw", -20, -10, rules)).toEqual({ x: 80, y: 90, width: 220, height: 110 });
    expect(dragFrame(box, "w", 500, 0, rules)).toEqual({ x: 296, y: 100, width: 4, height: 100 });
    expect(dragFrame(box, "s", 0, 5000, rules).height).toBe(692);
  });

  it("keeps a picture's proportions from a corner, following the bigger movement", () => {
    const resized = dragFrame(box, "se", 100, 10, { ...rules, keepAspect: true });
    expect(resized).toEqual({ x: 100, y: 100, width: 300, height: 150 });
    // the opposite corner stays where it was
    const fromTopLeft = dragFrame(box, "nw", 40, 0, { ...rules, keepAspect: true });
    expect(fromTopLeft.x + fromTopLeft.width).toBeCloseTo(300);
    expect(fromTopLeft.y + fromTopLeft.height).toBeCloseTo(200);
    expect(fromTopLeft.width / fromTopLeft.height).toBeCloseTo(2);
  });

  it("keeps the proportions from an edge too, around the middle of the other side", () => {
    const resized = dragFrame(box, "e", 100, 0, { ...rules, keepAspect: true });
    expect(resized.width).toBeCloseTo(300);
    expect(resized.height).toBeCloseTo(150);
    expect(resized.y + resized.height / 2).toBeCloseTo(150);
  });

  it("stops a picture at the page's edge with its proportions kept", () => {
    const resized = dragFrame(box, "se", 5000, 0, { ...rules, keepAspect: true });
    expect(resized.x + resized.width).toBeLessThanOrEqual(612.0001);
    expect(resized.width / resized.height).toBeCloseTo(2);
  });

  it("never makes a picture smaller than the smallest size", () => {
    const resized = dragFrame(box, "se", -5000, -5000, { ...rules, keepAspect: true });
    expect(resized.height).toBeCloseTo(4);
    expect(resized.width).toBeCloseTo(8);
  });

  it("shows eight handles, four for pictures, none when it can't be resized", () => {
    expect(handlesFor(true, false)).toEqual(HANDLES);
    expect(handlesFor(true, true)).toEqual(["nw", "ne", "se", "sw"]);
    expect(handlesFor(false, false)).toEqual([]);
  });
});

describe("the keyboard", () => {
  it("moves 1 point per arrow, 10 with shift", () => {
    expect(arrowStep("ArrowLeft", false)).toEqual([-1, 0]);
    expect(arrowStep("ArrowDown", true)).toEqual([0, 10]);
    expect(arrowStep("Enter", false)).toBeNull();
    const moved = frameAfterKey(box, "ArrowRight", { shift: true, alt: false }, { ...rules, resizable: true });
    expect(moved).toEqual({ x: 110, y: 100, width: 200, height: 100 });
  });

  it("resizes with alt, unless the object can't be resized", () => {
    const wider = frameAfterKey(box, "ArrowRight", { shift: false, alt: true }, { ...rules, resizable: true });
    expect(wider).toEqual({ x: 100, y: 100, width: 201, height: 100 });
    const fixed = frameAfterKey(box, "ArrowRight", { shift: false, alt: true }, { ...rules, resizable: false });
    expect(fixed).toEqual(box);
    expect(frameAfterKey(box, "x", { shift: false, alt: false }, { ...rules, resizable: true })).toBeNull();
  });

  it("compares rectangles to a hundredth of a point and names cursors", () => {
    expect(sameRect(box, { ...box, x: 100.001 })).toBe(true);
    expect(sameRect(box, { ...box, x: 101 })).toBe(false);
    expect(cursorFor("ne")).toBe("nesw-resize");
    expect(cursorFor("move")).toBe("move");
  });
});

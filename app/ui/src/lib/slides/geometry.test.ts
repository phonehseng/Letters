// the canvas maths: fitting, tiles, hit testing turned objects, turning,
// snapping and resizing a turned box.

import { describe, expect, it } from "vitest";
import { boundsOf, centredBox, contains, fitScale, objectAt, resizeTurned, rotationFor, snappedMove, tilesFor, turnPoint } from "./geometry";

const SLIDE = { width: 960, height: 540 };

describe("the slide canvas's maths", () => {
  it("fits the slide in the space it has", () => {
    expect(fitScale(SLIDE, { width: 480, height: 1000 })).toBeCloseTo(0.5);
    expect(fitScale(SLIDE, { width: 5000, height: 270 })).toBeCloseTo(0.5);
    expect(fitScale(SLIDE, { width: 0, height: 0 })).toBeCloseTo(0.1);
  });

  it("asks for the tiles a sharp picture needs, trimmed at the edges", () => {
    const { scaleMilli, tiles } = tilesFor(SLIDE, 1, 2);
    expect(scaleMilli).toBe(2000);
    // 1920 by 1080 device pixels is 4 by 3 tiles of 512
    expect(tiles.length).toBe(12);
    const last = tiles[tiles.length - 1];
    expect(last.width).toBe((1920 - 3 * 512) / 2);
    expect(last.height).toBe((1080 - 2 * 512) / 2);
  });

  it("finds the frontmost object under the pointer, turned ones too", () => {
    const back = { id: 1, x: 0, y: 0, width: 400, height: 400, rotation: 0 };
    const front = { id: 2, x: 100, y: 100, width: 100, height: 20, rotation: 90 };
    expect(objectAt([back, front], { x: 150, y: 160 })?.id).toBe(2);
    // the turned box stands upright now, so a point beside its old ends misses it
    expect(objectAt([back, front], { x: 195, y: 110 })?.id).toBe(1);
    expect(contains(front, { x: 150, y: 70 })).toBe(true);
  });

  it("turns in 15 degree steps unless Shift frees it", () => {
    const box = { x: 0, y: 0, width: 100, height: 100 };
    expect(rotationFor(box, { x: 50, y: -100 }, false)).toBe(0);
    expect(rotationFor(box, { x: 200, y: 50 }, false)).toBe(90);
    expect(rotationFor(box, { x: 200, y: 47 }, false)).toBe(90);
    expect(rotationFor(box, { x: 200, y: 47 }, true)).not.toBe(90);
    expect(rotationFor(box, { x: -100, y: 50 }, false)).toBe(270);
  });

  it("snaps a moved box to the slide's centre and to other objects", () => {
    const start = { x: 100, y: 100, width: 200, height: 100 };
    const centred = snappedMove(SLIDE, start, 278, 0, [], 1);
    expect(centred.box.x + centred.box.width / 2).toBe(480);
    expect(centred.guides.some((guide) => guide.vertical && guide.position === 480)).toBe(true);
    const beside = snappedMove(SLIDE, start, 0, 3, [{ x: 600, y: 205, width: 50, height: 50 }], 1);
    expect(beside.box.y + beside.box.height).toBe(205);
    const far = snappedMove(SLIDE, start, 37, 41, [], 1);
    expect(far.box).toEqual({ x: 137, y: 141, width: 200, height: 100 });
  });

  it("resizing a turned box keeps the opposite corner in place", () => {
    const start = { x: 100, y: 100, width: 200, height: 100, rotation: 30 };
    const centre = { x: 200, y: 150 };
    const corner = turnPoint({ x: 100, y: 100 }, centre, 30);
    const grown = resizeTurned(start, "se", 40, 10, false);
    const after = turnPoint({ x: grown.x, y: grown.y }, { x: grown.x + grown.width / 2, y: grown.y + grown.height / 2 }, 30);
    expect(after.x).toBeCloseTo(corner.x, 0);
    expect(after.y).toBeCloseTo(corner.y, 0);
    expect(grown.width).toBeGreaterThan(200);
  });

  it("an upright box resized from the left keeps its right edge", () => {
    const grown = resizeTurned({ x: 100, y: 100, width: 200, height: 100, rotation: 0 }, "w", -50, 0, false);
    expect(grown.x).toBe(50);
    expect(grown.x + grown.width).toBe(300);
  });

  it("puts new objects in the middle and bounds selections", () => {
    const box = centredBox(SLIDE, 2);
    expect(box.x + box.width / 2).toBe(480);
    expect(box.y + box.height / 2).toBe(270);
    expect(boundsOf([{ x: 10, y: 20, width: 5, height: 5 }, { x: 0, y: 30, width: 50, height: 1 }])).toEqual({ x: 0, y: 20, width: 50, height: 11 });
  });
});

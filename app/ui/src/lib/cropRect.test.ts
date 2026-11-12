import { describe, expect, it } from "vitest";
import { coversPage, dragCrop, keepOnPage, roundPoints, startingCrop } from "./cropRect";

const letter = { width: 612, height: 792 };

describe("the crop rectangle", () => {
  it("starts half an inch in from every edge", () => {
    expect(startingCrop(letter)).toEqual({ left: 36, top: 36, width: 540, height: 720 });
    // a tiny page still gets a sensible rectangle
    expect(startingCrop({ width: 40, height: 40 })).toEqual({ left: 10, top: 10, width: 20, height: 20 });
  });

  it("moves as a whole but never leaves the page", () => {
    const start = { left: 36, top: 36, width: 100, height: 100 };
    expect(dragCrop(start, "move", 10, 20, letter)).toEqual({ left: 46, top: 56, width: 100, height: 100 });
    expect(dragCrop(start, "move", -500, 5000, letter)).toEqual({ left: 0, top: 692, width: 100, height: 100 });
  });

  it("drags a corner or an edge without crossing the opposite side", () => {
    const start = { left: 100, top: 100, width: 200, height: 200 };
    expect(dragCrop(start, "se", 50, 30, letter)).toEqual({ left: 100, top: 100, width: 250, height: 230 });
    expect(dragCrop(start, "w", 500, 0, letter)).toEqual({ left: 297, top: 100, width: 3, height: 200 });
    expect(dragCrop(start, "n", 0, -500, letter)).toEqual({ left: 100, top: 0, width: 200, height: 300 });
    expect(dragCrop(start, "e", 1000, 0, letter).width).toBe(512);
  });

  it("pulls typed numbers back onto the page", () => {
    expect(keepOnPage({ left: -5, top: 700, width: 1000, height: 200 }, letter)).toEqual({
      left: 0,
      top: 592,
      width: 612,
      height: 200,
    });
    expect(keepOnPage({ left: 0, top: 0, width: 1, height: 1 }, letter)).toMatchObject({ width: 3, height: 3 });
  });

  it("knows when nothing would be hidden", () => {
    expect(coversPage({ left: 0, top: 0, width: 612, height: 792 }, letter)).toBe(true);
    expect(coversPage(startingCrop(letter), letter)).toBe(false);
    expect(roundPoints(12.345)).toBe(12.3);
  });
});

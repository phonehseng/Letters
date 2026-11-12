import { describe, expect, it } from "vitest";
import {
  autoScrollStep,
  changesNothing,
  dropIndexAt,
  insertionLineY,
  orderAfterMove,
  positionsAfterMove,
  stepTarget,
  type Slot,
} from "./movePages";

describe("orderAfterMove", () => {
  it("moves a block down, keeping its order", () => {
    expect(orderAfterMove(6, [1, 2], 5)).toEqual([0, 3, 4, 1, 2, 5]);
  });

  it("moves scattered pages up together", () => {
    expect(orderAfterMove(6, [4, 2], 0)).toEqual([2, 4, 0, 1, 3, 5]);
  });

  it("drops at the end", () => {
    expect(orderAfterMove(4, [0], 4)).toEqual([1, 2, 3, 0]);
  });

  it("treats a drop onto a moving page as before the next page that stays", () => {
    expect(orderAfterMove(5, [1, 3], 3)).toEqual([0, 2, 1, 3, 4]);
  });

  it("knows where the moved pages end up", () => {
    expect(positionsAfterMove(6, [1, 2], 5)).toEqual([3, 4]);
  });

  it("spots drops that change nothing", () => {
    expect(changesNothing(5, [1, 2], 1)).toBe(true);
    expect(changesNothing(5, [1, 2], 3)).toBe(true);
    expect(changesNothing(5, [1, 2], 4)).toBe(false);
  });
});

describe("stepTarget", () => {
  it("moves one place up or down, and stops at the edges", () => {
    expect(stepTarget([2, 3], "up", 6)).toBe(1);
    expect(orderAfterMove(6, [2, 3], 1)).toEqual([0, 2, 3, 1, 4, 5]);
    expect(stepTarget([2, 3], "down", 6)).toBe(5);
    expect(orderAfterMove(6, [2, 3], 5)).toEqual([0, 1, 4, 2, 3, 5]);
    expect(stepTarget([3], "down", 5)).toBe(5);
    expect(stepTarget([0], "up", 5)).toBeNull();
    expect(stepTarget([4], "down", 5)).toBeNull();
    expect(stepTarget([], "up", 5)).toBeNull();
  });
});

describe("dragging in a virtualised list", () => {
  // 500 thumbnails of 200 px each with 16 px gaps, starting at 16
  const slots: Slot[] = Array.from({ length: 500 }, (_, index) => ({ top: 16 + index * 216, height: 200 }));

  it("finds the insertion point from the pointer alone", () => {
    expect(dropIndexAt(slots, 0)).toBe(0);
    expect(dropIndexAt(slots, 100)).toBe(0);
    expect(dropIndexAt(slots, 130)).toBe(1);
    expect(dropIndexAt(slots, 216 * 250 + 16 + 150)).toBe(251);
    expect(dropIndexAt(slots, 10_000_000)).toBe(500);
  });

  it("draws the line in the gap above the slot, or under the last", () => {
    expect(insertionLineY(slots, 1, 16)).toBe(224);
    expect(insertionLineY(slots, 500, 16)).toBe(16 + 499 * 216 + 200 + 8);
    expect(insertionLineY([], 0, 16)).toBe(0);
  });

  it("scrolls faster the closer the pointer is to an edge", () => {
    expect(autoScrollStep(200, 400)).toBe(0);
    expect(autoScrollStep(0, 400)).toBe(-18);
    expect(autoScrollStep(40, 400)).toBeLessThan(0);
    expect(autoScrollStep(399, 400)).toBeGreaterThan(0);
  });
});

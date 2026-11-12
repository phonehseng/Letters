// tests of the grid's sizes, selection rules and keys.

import { describe, expect, it } from "vitest";
import { cellName, columnCharacters, columnIndex, columnLines, columnName, columnPixels, Lines, rowLines } from "./geometry";
import { fillTarget } from "./gridHit";
import { gridKeyAction, startsTyping } from "./gridKeys";
import { addCell, extendTo, MAX_COLS, MAX_ROWS, movingCorner, nextInSelection, parseRangeName, rangeName, rangeOf, single, wholeColumns } from "./selection";
import type { KeyPress } from "../commands";

describe("sizes of rows and columns", () => {
  it("measures excel's default column as 64 pixels and back", () => {
    expect(columnPixels(8.43)).toBe(64);
    expect(columnCharacters(64)).toBeCloseTo(8.43, 1);
  });

  it("finds lines in a sheet with one tall row and a hidden one", () => {
    const lines = new Lines(20, new Map([[3, 50], [5, 0]]), 200_000);
    expect(lines.offsetOf(3)).toBe(60);
    expect(lines.offsetOf(4)).toBe(110);
    expect(lines.offsetOf(6)).toBe(130);
    expect(lines.indexAt(115)).toBe(4);
    expect(lines.indexAt(130)).toBe(6);
    expect(lines.total()).toBe(200_000 * 20 + 30 - 20);
    expect(lines.nextVisible(4, 1)).toBe(6);
  });

  it("finds a row among 200,000 without adding them up", () => {
    const lines = rowLines([{ first: 10, last: 10, size: 30, hidden: false }], 15, 200_000, 1);
    expect(lines.indexAt(lines.offsetOf(150_000) + 1)).toBe(150_000);
  });

  it("reads hidden and sized column runs", () => {
    const lines = columnLines([{ first: 1, last: 2, size: null, hidden: true }, { first: 3, last: 3, size: 20, hidden: false }], 8.43, 30, 1);
    expect(lines.sizeOf(1)).toBe(0);
    expect(lines.sizeOf(3)).toBe(145);
  });

  it("names columns and cells as excel does", () => {
    expect(columnName(0)).toBe("A");
    expect(columnName(25)).toBe("Z");
    expect(columnName(26)).toBe("AA");
    expect(columnName(16_383)).toBe("XFD");
    expect(columnIndex("xfd")).toBe(16_383);
    expect(cellName(2, 1, true)).toBe("$B$3");
  });
});

describe("selection", () => {
  it("extends from the anchor and adds ranges with ctrl", () => {
    const start = single({ row: 2, col: 2 });
    const wider = extendTo(start, { row: 0, col: 4 });
    expect(rangeName(wider.ranges[0])).toBe("C1:E3");
    expect(movingCorner(wider)).toEqual({ row: 0, col: 4 });
    const two = addCell(wider, { row: 9, col: 0 });
    expect(two.ranges).toHaveLength(2);
    expect(two.active).toEqual({ row: 9, col: 0 });
  });

  it("names and reads ranges, whole columns and rows", () => {
    expect(rangeName(wholeColumns(1, 2).ranges[0])).toBe("B:C");
    expect(parseRangeName("$b$3")).toEqual(rangeOf({ row: 2, col: 1 }, { row: 2, col: 1 }));
    expect(parseRangeName("4:9")?.end).toEqual({ row: 8, col: MAX_COLS - 1 });
    expect(parseRangeName("C:C")?.end.row).toBe(MAX_ROWS - 1);
    expect(parseRangeName("not a cell")).toBeNull();
  });

  it("enter walks down the columns of a selection and wraps", () => {
    const block = extendTo(single({ row: 0, col: 0 }), { row: 1, col: 1 });
    const steps = [1, 2, 3, 4].reduce<{ row: number; col: number }[]>((seen, _, i) => {
      const from = i === 0 ? block : { ...block, active: seen[i - 1] };
      return [...seen, nextInSelection(from, false, false).active];
    }, []);
    expect(steps).toEqual([{ row: 1, col: 0 }, { row: 0, col: 1 }, { row: 1, col: 1 }, { row: 0, col: 0 }]);
  });
});

function key(key: string, code: string, extra: Partial<KeyPress> = {}): KeyPress {
  return { key, code, ctrlKey: false, metaKey: false, altKey: false, shiftKey: false, ...extra };
}

describe("the grid's keys are excel's and google sheets'", () => {
  it("moves, jumps to the edge of data and extends", () => {
    expect(gridKeyAction(key("ArrowDown", "ArrowDown"))).toEqual({ kind: "move", direction: "down", extend: false });
    expect(gridKeyAction(key("ArrowRight", "ArrowRight", { ctrlKey: true, shiftKey: true }))).toEqual({ kind: "edge", direction: "right", extend: true });
    expect(gridKeyAction(key("ArrowUp", "ArrowUp", { metaKey: true }))).toEqual({ kind: "edge", direction: "up", extend: false });
  });

  it("formats with ctrl shift and a digit whatever the digit types", () => {
    expect(gridKeyAction(key("%", "Digit5", { ctrlKey: true, shiftKey: true }))).toMatchObject({ kind: "numberFormat", code: "0%" });
    expect(gridKeyAction(key("5", "Digit5", { ctrlKey: true }))).toEqual({ kind: "toggle", style: "strike" });
    expect(gridKeyAction(key("b", "KeyB", { metaKey: true }))).toEqual({ kind: "toggle", style: "bold" });
  });

  it("fills, sums, selects and changes sheets", () => {
    expect(gridKeyAction(key("d", "KeyD", { ctrlKey: true }))).toEqual({ kind: "fill", direction: "down" });
    expect(gridKeyAction(key("r", "KeyR", { ctrlKey: true }))).toEqual({ kind: "fill", direction: "right" });
    expect(gridKeyAction(key("=", "Equal", { altKey: true }))).toEqual({ kind: "autoSum" });
    expect(gridKeyAction(key(" ", "Space", { shiftKey: true }))).toEqual({ kind: "selectRow" });
    expect(gridKeyAction(key(" ", "Space", { ctrlKey: true }))).toEqual({ kind: "selectColumn" });
    expect(gridKeyAction(key("PageDown", "PageDown", { ctrlKey: true }))).toEqual({ kind: "sheet", step: 1 });
    expect(gridKeyAction(key("ArrowDown", "ArrowDown", { altKey: true }))).toEqual({ kind: "sheet", step: 1 });
  });

  it("leaves altgr characters for typing", () => {
    const at = key("@", "KeyQ", { ctrlKey: true, altKey: true, altGraph: true });
    expect(gridKeyAction(at)).toBeNull();
    expect(startsTyping(at)).toBe(true);
    expect(startsTyping(key("a", "KeyA"))).toBe(true);
    expect(startsTyping(key("a", "KeyA", { ctrlKey: true }))).toBe(false);
  });
});

describe("the fill handle", () => {
  const source = { start: { row: 1, col: 1 }, end: { row: 2, col: 1 } };

  it("fills in the one direction the pointer went furthest", () => {
    expect(fillTarget(source, { row: 9, col: 2 })).toEqual({ start: { row: 1, col: 1 }, end: { row: 9, col: 1 } });
    expect(fillTarget(source, { row: 2, col: 6 })).toEqual({ start: { row: 1, col: 1 }, end: { row: 2, col: 6 } });
    expect(fillTarget(source, { row: 0, col: 1 })).toEqual({ start: { row: 0, col: 1 }, end: { row: 2, col: 1 } });
  });

  it("fills nothing while the pointer stays inside the selection", () => {
    expect(fillTarget(source, { row: 2, col: 1 })).toBeNull();
  });
});

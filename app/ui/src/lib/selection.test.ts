import { describe, expect, it } from "vitest";
import {
  clickPage,
  extendTo,
  keepWithin,
  moveByKey,
  NO_SELECTION,
  selectAll,
  selectOnly,
  targetPages,
  toggle,
} from "./selection";

const plain = { toggle: false, extend: false };

describe("clicking thumbnails", () => {
  it("selects one page with a plain click", () => {
    expect(clickPage(selectAll(5), 2, plain)).toEqual({ pages: [2], anchor: 2, focus: 2 });
  });

  it("adds and removes pages with ctrl or command click", () => {
    let selection = selectOnly(1);
    selection = clickPage(selection, 4, { toggle: true, extend: false });
    expect(selection.pages).toEqual([1, 4]);
    selection = toggle(selection, 1);
    expect(selection.pages).toEqual([4]);
  });

  it("selects a run from the anchor with shift click, either way", () => {
    expect(extendTo(selectOnly(2), 5).pages).toEqual([2, 3, 4, 5]);
    expect(extendTo(selectOnly(5), 2).pages).toEqual([2, 3, 4, 5]);
    // a second shift click re-measures from the same anchor
    const twice = clickPage(clickPage(selectOnly(3), 6, { toggle: false, extend: true }), 1, { toggle: false, extend: true });
    expect(twice).toEqual({ pages: [1, 2, 3], anchor: 3, focus: 1 });
  });

  it("starts a shift click run at the page itself when nothing was clicked", () => {
    expect(extendTo(NO_SELECTION, 3).pages).toEqual([3]);
  });
});

describe("keys", () => {
  it("selects every page", () => {
    expect(selectAll(3).pages).toEqual([0, 1, 2]);
    expect(selectAll(0)).toEqual(NO_SELECTION);
  });

  it("moves the selection with the arrows and stops at the ends", () => {
    expect(moveByKey(selectOnly(2), "ArrowDown", 5, false, 0)?.pages).toEqual([3]);
    expect(moveByKey(selectOnly(0), "ArrowUp", 5, false, 0)?.pages).toEqual([0]);
    expect(moveByKey(selectOnly(2), "End", 5, false, 0)?.pages).toEqual([4]);
    expect(moveByKey(selectOnly(2), "Enter", 5, false, 0)).toBeNull();
  });

  it("grows the selection with shift and the arrows", () => {
    const grown = moveByKey(moveByKey(selectOnly(2), "ArrowDown", 5, true, 0)!, "ArrowDown", 5, true, 0);
    expect(grown?.pages).toEqual([2, 3, 4]);
    expect(moveByKey(grown!, "ArrowUp", 5, true, 0)?.pages).toEqual([2, 3]);
  });

  it("starts from the page being read when nothing is selected", () => {
    expect(moveByKey(NO_SELECTION, "ArrowDown", 5, false, 3)?.pages).toEqual([4]);
  });
});

describe("after an edit", () => {
  it("forgets pages that no longer exist", () => {
    expect(keepWithin({ pages: [1, 3, 4], anchor: 4, focus: 3 }, 4)).toEqual({ pages: [1, 3], anchor: null, focus: 3 });
  });
});

describe("targetPages", () => {
  it("uses the selection, or the page being read when there is none", () => {
    expect(targetPages({ pages: [1, 2], anchor: 1, focus: 2 }, 7)).toEqual({ pages: [1, 2], fromSelection: true });
    expect(targetPages(NO_SELECTION, 7)).toEqual({ pages: [7], fromSelection: false });
  });
});

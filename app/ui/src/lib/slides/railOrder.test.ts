// the slide rail's drops, selections and deletions, and the layout New
// Slide picks.

import { describe, expect, it } from "vitest";
import { deleteOperations, dropGap, dropOperation, newSlideLayout, railSelection } from "./railOrder";

describe("the slide rail", () => {
  it("drops a dragged slide into the gap nearest the pointer", () => {
    expect(dropGap(10, [50, 150, 250])).toBe(0);
    expect(dropGap(160, [50, 150, 250])).toBe(2);
    expect(dropGap(900, [50, 150, 250])).toBe(3);
  });

  it("moves a slide only when it lands somewhere new", () => {
    expect(dropOperation(0, 0)).toBeNull();
    expect(dropOperation(0, 1)).toBeNull();
    expect(dropOperation(0, 3)).toEqual({ op: "moveSlide", from: 0, to: 2 });
    expect(dropOperation(2, 0)).toEqual({ op: "moveSlide", from: 2, to: 0 });
  });

  it("selects with Shift and Ctrl clicks as a list does", () => {
    const ids = ["a", "b", "c", "d"];
    expect(railSelection(ids, ["a"], 0, 2, true, false)).toEqual(["a", "b", "c"]);
    expect(railSelection(ids, ["a"], 0, 3, false, true)).toEqual(["a", "d"]);
    expect(railSelection(ids, ["a", "d"], 0, 3, false, true)).toEqual(["a"]);
    expect(railSelection(ids, ["a"], 0, 0, false, true)).toEqual(["a"]);
    expect(railSelection(ids, ["a", "b"], 0, 2, false, false)).toEqual(["c"]);
  });

  it("deletes from the last slide back, and never the last one left", () => {
    expect(deleteOperations(["a", "b", "c"], ["a", "c"])).toEqual([
      { op: "deleteSlide", index: 2 },
      { op: "deleteSlide", index: 0 },
    ]);
    expect(deleteOperations(["a", "b"], ["a", "b"])).toHaveLength(1);
  });
});

describe("New Slide", () => {
  const layouts = [
    { id: "1", name: "Title Slide" },
    { id: "2", name: "Title and Content" },
    { id: "3", name: "Two Content" },
  ];

  it("follows a title slide with title and content, and other slides with their own layout", () => {
    expect(newSlideLayout(layouts, "1")).toBe("2");
    expect(newSlideLayout(layouts, "3")).toBe("3");
    expect(newSlideLayout(layouts, null)).toBe("2");
    expect(newSlideLayout([], null)).toBeNull();
  });
});

import { describe, expect, it } from "vitest";
import type { OutlineItem } from "./api";
import { currentOutlineIndex, treePositions } from "./outline";

const item = (title: string, page: number, level: number): OutlineItem => ({ title, page, level });

const book = [
  item("Preface", 1, 0),
  item("Chapter 1", 4, 0),
  item("1.1 Cells", 5, 1),
  item("1.2 Tissues", 9, 1),
  item("Chapter 2", 14, 0),
];

describe("currentOutlineIndex", () => {
  it("picks the last entry starting at or before the current page", () => {
    expect(currentOutlineIndex(book, 4)).toBe(1);
    expect(currentOutlineIndex(book, 7)).toBe(2);
    expect(currentOutlineIndex(book, 13)).toBe(3);
    expect(currentOutlineIndex(book, 300)).toBe(4);
  });

  it("has no current entry before the first one or without an outline", () => {
    expect(currentOutlineIndex(book, 0)).toBe(-1);
    expect(currentOutlineIndex([], 3)).toBe(-1);
  });
});

describe("treePositions", () => {
  it("counts siblings within each parent", () => {
    expect(treePositions(book)).toEqual([
      { level: 1, positionInSet: 1, setSize: 3 },
      { level: 1, positionInSet: 2, setSize: 3 },
      { level: 2, positionInSet: 1, setSize: 2 },
      { level: 2, positionInSet: 2, setSize: 2 },
      { level: 1, positionInSet: 3, setSize: 3 },
    ]);
  });

  it("starts a new group under each parent", () => {
    const positions = treePositions([item("A", 0, 0), item("a", 0, 1), item("B", 0, 0), item("b", 0, 1)]);
    expect(positions[3]).toEqual({ level: 2, positionInSet: 1, setSize: 1 });
  });

  it("treats a jump of several levels as one", () => {
    const positions = treePositions([item("Top", 0, 0), item("Deep", 0, 3)]);
    expect(positions[1].level).toBe(2);
  });
});

import { describe, expect, it } from "vitest";
import { fileDropTarget, pdfsAmong, setFileDropTarget, toCssPoint, type FileDropTarget } from "./fileDrop";

const target = (): FileDropTarget => ({ over: () => false, drop: () => false, leave: () => {} });

describe("file drops", () => {
  it("turns physical pixels into css pixels", () => {
    expect(toCssPoint({ x: 300, y: 150 }, 1.5)).toEqual({ x: 200, y: 100 });
    expect(toCssPoint(undefined, 2)).toBeNull();
    expect(toCssPoint({ x: Number.NaN, y: 1 }, 1)).toBeNull();
  });

  it("only inserts pdfs", () => {
    expect(pdfsAmong(["a.PDF", "b.png", "c.pdf"])).toEqual(["a.PDF", "c.pdf"]);
  });

  it("keeps a newer target when an older one lets go late", () => {
    const first = target();
    const second = target();
    const takeBackFirst = setFileDropTarget(first);
    setFileDropTarget(second);
    takeBackFirst();
    expect(fileDropTarget()).toBe(second);
  });
});

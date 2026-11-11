import { describe, expect, it } from "vitest";
import { hiddenParts, NARROWING } from "./toolbar";

describe("hiddenParts", () => {
  it("hides nothing on a wide window or before it is measured", () => {
    expect(hiddenParts(1200, 16).size).toBe(0);
    expect(hiddenParts(0, 16).size).toBe(0);
  });

  it("drops the words before any button", () => {
    expect([...hiddenParts(800, 16)]).toEqual(["labels"]);
  });

  it("keeps hiding in order at the minimum window width", () => {
    // 480 px is 30 rem: words, percentage, new window and fit width are gone
    expect([...hiddenParts(480, 16)]).toEqual(["labels", "zoomLevel", "newWindow", "pageWord", "fitWidth"]);
  });

  it("hides more when a side panel leaves the document a strip", () => {
    const hidden = hiddenParts(250, 16);
    expect(hidden.has("zoomButtons")).toBe(true);
    expect(hidden.has("find")).toBe(true);
  });

  it("lists the rules from widest to narrowest", () => {
    const widths = NARROWING.map((rule) => rule.belowRem);
    expect([...widths].sort((a, b) => b - a)).toEqual(widths);
  });
});

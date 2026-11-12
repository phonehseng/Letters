import { describe, expect, it } from "vitest";
import { closePanel, fitPanels, inspectorFloats, openPanel, togglePanel } from "./sidePanels";

const rem = 16;
const wide = 1600; // 100 rem: every panel fits
const medium = 1000; // 62.5 rem: two panels fit, not three
const narrow = 480; // 30 rem: the smallest window

describe("side panels", () => {
  it("lets all three panels show on a wide window", () => {
    let open = openPanel([], "sidebar", wide, rem);
    open = openPanel(open, "inspector", wide, rem);
    open = openPanel(open, "explain", wide, rem);
    expect(open).toEqual(["sidebar", "inspector", "explain"]);
  });

  it("closes the panel opened longest ago when the newest doesn't fit", () => {
    let open = openPanel([], "sidebar", medium, rem);
    open = openPanel(open, "inspector", medium, rem);
    open = openPanel(open, "explain", medium, rem);
    expect(open).toEqual(["inspector", "explain"]);
  });

  it("keeps the newest panels when the window shrinks", () => {
    expect(fitPanels(["sidebar", "inspector", "explain"], medium, rem)).toEqual(["inspector", "explain"]);
  });

  it("floats the inspector over the document on a small window, alone", () => {
    expect(inspectorFloats(narrow, rem)).toBe(true);
    expect(inspectorFloats(wide, rem)).toBe(false);
    expect(openPanel(["sidebar"], "inspector", narrow, rem)).toEqual(["inspector"]);
    expect(openPanel(["inspector"], "explain", narrow, rem)).toEqual(["explain"]);
  });

  it("toggles and closes", () => {
    expect(togglePanel(["sidebar"], "sidebar", wide, rem)).toEqual([]);
    expect(togglePanel([], "explain", wide, rem)).toEqual(["explain"]);
    expect(closePanel(["sidebar", "explain"], "sidebar")).toEqual(["explain"]);
  });
});

import { describe, expect, it } from "vitest";
import { CLOSED_SIDEBAR, onlyOnePanelFits, toggleSidebar, toggleSidebarMode } from "./sidebar";

describe("sidebar state", () => {
  it("starts closed on thumbnails", () => {
    expect(CLOSED_SIDEBAR).toEqual({ open: false, mode: "thumbnails" });
  });

  it("opens and closes from the toolbar keeping the last mode", () => {
    const open = toggleSidebar({ open: false, mode: "outline" });
    expect(open).toEqual({ open: true, mode: "outline" });
    expect(toggleSidebar(open)).toEqual({ open: false, mode: "outline" });
  });

  it("switches mode, or closes when that mode is already showing", () => {
    expect(toggleSidebarMode(CLOSED_SIDEBAR, "outline")).toEqual({ open: true, mode: "outline" });
    expect(toggleSidebarMode({ open: true, mode: "thumbnails" }, "outline")).toEqual({
      open: true,
      mode: "outline",
    });
    expect(toggleSidebarMode({ open: true, mode: "outline" }, "outline")).toEqual({
      open: false,
      mode: "outline",
    });
  });
});

describe("onlyOnePanelFits", () => {
  it("is true on narrow windows only", () => {
    expect(onlyOnePanelFits(480, 16)).toBe(true);
    expect(onlyOnePanelFits(1024, 16)).toBe(false);
    // a larger system text size makes the same window count as narrower
    expect(onlyOnePanelFits(900, 20)).toBe(true);
  });
});

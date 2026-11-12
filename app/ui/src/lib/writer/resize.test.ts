import { describe, expect, it } from "vitest";
import { fitWidth, freeOffsets, nudgeSize, previewOrigin, resize, snapPosition } from "./resize";
import { formatForPath, saveFilters, SAVE_FORMATS } from "./formats";
import { chooseProvider, layoutPages, pagesFromWord, providerExplanation, providerLabel } from "./layoutSource";
import { compatibilityLine, gate } from "./strictMode";
import { printLayout, pageRule } from "./print";
import { metricSubstitute } from "./fonts";

const start = { width: 200, height: 100 };

describe("resizing pictures", () => {
  it("corner handles keep proportions unless Shift unlocks them", () => {
    expect(resize(start, "se", 100, 0, { lockAspect: true, fromCentre: false })).toEqual({ width: 300, height: 150, snappedTo: null });
    expect(resize(start, "se", 100, 10, { lockAspect: false, fromCentre: false })).toEqual({ width: 300, height: 110, snappedTo: null });
    expect(resize(start, "nw", 50, 0, { lockAspect: true, fromCentre: false })).toEqual({ width: 150, height: 75, snappedTo: null });
  });

  it("side handles drive their own side; Alt grows from the centre", () => {
    expect(resize(start, "s", 0, 50, { lockAspect: true, fromCentre: false })).toMatchObject({ width: 300, height: 150 });
    expect(resize(start, "e", 50, 0, { lockAspect: true, fromCentre: true })).toMatchObject({ width: 300, height: 150 });
    expect(previewOrigin("se", false)).toBe("0% 0%");
    expect(previewOrigin("nw", true)).toBe("50% 50%");
  });

  it("snaps to the original size or the text width and says which", () => {
    expect(resize(start, "e", 96, 0, { lockAspect: true, fromCentre: false, snapWidths: [300] })).toEqual({ width: 300, height: 150, snappedTo: 300 });
    expect(resize(start, "e", -500, 0, { lockAspect: false, fromCentre: false, minimum: 8 }).width).toBe(8);
  });

  it("fit width, keyboard steps and free positions", () => {
    expect(fitWidth({ width: 1000, height: 500 }, 400)).toEqual({ width: 400, height: 200 });
    expect(fitWidth(start, 400)).toEqual(start);
    expect(nudgeSize(start, 1, 20)).toEqual({ width: 220, height: 110 });
    expect(snapPosition(3, 100, 600)).toEqual({ left: 0, guide: { axis: "x", at: 0 } });
    expect(snapPosition(248, 100, 600)).toEqual({ left: 250, guide: { axis: "x", at: 300 } });
    expect(snapPosition(120, 100, 600).guide).toBeNull();
    // 1.5x zoom: 15 twips per css pixel, so 0.1 px per twip
    expect(freeOffsets({ left: 160, top: 230 }, { left: 100, top: 200 }, 1.5 / 15)).toEqual({ horizontal: 600, vertical: 300 });
  });
});

describe("save formats", () => {
  it("the name chooses the format; Word and OpenDocument become the document's file", () => {
    expect(formatForPath("C:\\a\\essay.docx")?.becomes).toBe("own");
    expect(formatForPath("essay.HTML")?.id).toBe("html");
    expect(formatForPath("essay.pdf")?.becomes).toBe("copy");
    expect(formatForPath("essay.rtf")).toBeNull();
    expect(saveFilters("odt")[0]).toEqual({ name: "OpenDocument Text", extensions: ["odt"] });
    expect(SAVE_FORMATS.every((format) => format.note.endsWith("."))).toBe(true);
  });
});

describe("where page breaks come from", () => {
  it("Word only in strict mode and when installed, then the engine, then the estimate", () => {
    expect(chooseProvider("strict", { word: true, engine: false })).toBe("word");
    expect(chooseProvider("strict", { word: false, engine: false })).toBe("estimate");
    expect(chooseProvider("free", { word: true, engine: true })).toBe("engine");
    expect(chooseProvider("free", { word: true, engine: false })).toBe("estimate");
  });

  it("Word's pages become page starts, and a mismatch falls back to the estimate", () => {
    expect(pagesFromWord({ blockPages: [1, 1, 2, 2, 3], pageCount: 3 }, 0).starts).toEqual([0, 2, 4]);
    const boxes = [{ height: 10 }, { height: 10 }, { height: 10 }];
    expect(layoutPages("word", boxes, 900, { blockPages: [1, 2, 2], pageCount: 2 }).starts).toEqual([0, 1]);
    expect(layoutPages("word", boxes, 900, { blockPages: [], pageCount: 2 }).starts).toEqual([0]);
  });

  it("says plainly where the pages come from", () => {
    expect(providerLabel("word", { checking: true, checkedAt: null, wordInstalled: true }, 0)).toBe("Checking layout with Word…");
    expect(providerLabel("word", { checking: false, checkedAt: 1000, wordInstalled: true }, 5000)).toBe("Layout checked with Word a moment ago");
    expect(providerLabel("estimate", { checking: false, checkedAt: null, wordInstalled: false }, 0)).toBe("Pages estimated");
    expect(providerExplanation("estimate", false)).toBe("Page breaks are estimated because Microsoft Word isn’t installed on this computer.");
    expect(providerExplanation("word", true)).toContain("not an imitation");
  });
});

describe("strict word mode", () => {
  it("turns off what Word would show differently, with the reason", () => {
    expect(gate("wColumns", "free").enabled).toBe(true);
    const gated = gate("wColumns", "strict");
    expect(gated.enabled).toBe(false);
    expect(gated.reason).toContain("Strict Word mode");
    expect(gate("wBold", "strict").enabled).toBe(true);
    expect(compatibilityLine([])).toBe("Everything here opens the same in Word.");
    expect(compatibilityLine([{ what: "a" }, { what: "b" }])).toBe("2 things will look different in Word");
  });
});

describe("printing", () => {
  it("pages print at their own size with tiles at 300 dpi", () => {
    const [page] = printLayout(7, [{ width: 612, height: 792 }], 3, (id, index, scale, column, row, revision) => `${id}/${index}/${scale}/${column}/${row}?r=${revision}`);
    expect(page.width).toBe(816);
    expect(page.height).toBe(1056);
    // 612 pt at 300 dpi is 2550 device pixels: five 512 pixel tiles across
    expect(new Set(page.tiles.map((tile) => tile.left)).size).toBe(5);
    expect(page.tiles[0].src).toBe("7/0/4167/0/0?r=3");
    expect(pageRule({ width: 612, height: 792 })).toBe("@page { size: 612pt 792pt; margin: 0 }");
  });
});

describe("fonts", () => {
  it("missing Office fonts get the open ones with the same widths", () => {
    expect(metricSubstitute("Calibri")).toBe("Carlito");
    expect(metricSubstitute("Times New Roman")).toBe("Tinos");
    expect(metricSubstitute("Comic Sans MS")).toBeNull();
  });
});

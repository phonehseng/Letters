// resizing pictures, the Save As formats, where page breaks come from, strict mode and printing
import { describe, expect, it } from "vitest";
import { fitWidth, freeOffsets, nudgeSize, previewOrigin, resize, snapPosition } from "./resize";
import { copySavedSentence, formatForPath, saveFilters, saveFormatsFrom } from "./formats";
import { chooseProvider, layoutPages, pagesFromWord, providerExplanation, providerLabel } from "./layoutSource";
import { compatibilityLine, fixableCount, fixLabel, gate, switchQuestion, wordExact } from "./strictMode";
import type { CompatibilityIssue, EngineFeature, FormatCapability } from "./writerApi";
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

// the engine's table as formats::format_capabilities gives it (core/office/src/formats.rs)
const ENGINE_FORMATS: FormatCapability[] = [
  { format: "docx", label: "Word Document", extension: "docx", canOpen: true, canSave: true, needsConverter: false, summary: "Keeps everything." },
  { format: "odt", label: "OpenDocument Text", extension: "odt", canOpen: true, canSave: true, needsConverter: false, summary: "Keeps text and styles." },
  { format: "rtf", label: "Rich Text Format", extension: "rtf", canOpen: true, canSave: true, needsConverter: false, summary: "Keeps formatting." },
  { format: "markdown", label: "Markdown", extension: "md", canOpen: false, canSave: true, needsConverter: false, summary: "Keeps headings." },
  { format: "html", label: "Web Page", extension: "html", canOpen: false, canSave: true, needsConverter: false, summary: "Keeps links." },
  { format: "doc", label: "Word 97–2003 Document", extension: "doc", canOpen: true, canSave: true, needsConverter: true, summary: "For old Word." },
];

describe("save formats", () => {
  it("come from the engine's table; the name chooses the format; Word and OpenDocument become the document's file", () => {
    const formats = saveFormatsFrom(ENGINE_FORMATS, false);
    expect(formatForPath("C:\\a\\essay.docx", formats)?.becomes).toBe("own");
    expect(formatForPath("essay.HTML", formats)?.id).toBe("html");
    expect(formatForPath("essay.pdf", formats)?.becomes).toBe("copy");
    expect(formatForPath("essay.rtf", formats)?.note).toBe("Keeps formatting.");
    expect(formatForPath("essay.md", formats)?.id).toBe("md");
    // .doc needs Word or LibreOffice
    expect(formatForPath("essay.doc", formats)).toBeNull();
    expect(formatForPath("essay.doc", saveFormatsFrom(ENGINE_FORMATS, true))?.id).toBe("doc");
    expect(formats[2].id).toBe("pdf");
    expect(saveFilters("odt", formats)[0]).toEqual({ name: "OpenDocument Text", extensions: ["odt"] });
    expect(formats.every((format) => format.note.endsWith("."))).toBe(true);
    expect(copySavedSentence(formats.find((format) => format.id === "md")!, "Essay.docx")).toContain("images");
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
    expect(providerLabel("word", { checking: false, checkedAt: 0, wordInstalled: true }, 120_000)).toBe("Layout checked with Word 2 minutes ago");
    expect(providerLabel("word", { checking: false, checkedAt: 0, wordInstalled: true, stale: true }, 1000)).toBe("Layout checked with Word a moment ago, before your last edits");
    expect(providerLabel("word", { checking: false, checkedAt: null, wordInstalled: true, failed: "x" }, 0)).toBe("Word couldn’t check the layout");
    expect(providerLabel("estimate", { checking: false, checkedAt: null, wordInstalled: false }, 0)).toBe("Pages estimated");
    expect(providerExplanation("estimate", false)).toBe("Page breaks are estimated because Microsoft Word isn’t installed on this computer.");
    expect(providerExplanation("word", true)).toContain("half a minute");
  });
});

describe("strict word mode", () => {
  // a part of compat::feature_matrix() as the engine gives it
  const matrix: EngineFeature[] = [
    { key: "columns", label: "Text columns", support: "both", note: "" },
    { key: "highlight", label: "Highlighter", support: "wordExact", note: "word has 16 highlighter colours; odt any colour" },
    { key: "pictureCrop", label: "Cropped pictures", support: "wordExact", note: "odt keeps the whole picture" },
    { key: "lists", label: "Unusual lists", support: "approximate", note: "each format has its own set" },
  ];

  it("offers only what the engine's matrix calls Word-exact, with the engine's reason", () => {
    expect(gate("wColumns", "strict", matrix).enabled).toBe(true);
    expect(gate("wImageCrop", "strict", matrix).enabled).toBe(true);
    expect(gate("wHighlight", "strict", matrix).enabled).toBe(true);
    const gated = gate("wLists", "strict", matrix);
    expect(gated.enabled).toBe(false);
    expect(gated.reason).toBe("Off in Strict Word mode: Each format has its own set.");
    expect(gate("wLists", "free", matrix).enabled).toBe(true);
    expect(gate("wBold", "strict", matrix).enabled).toBe(true);
    expect(wordExact(matrix[1])).toBe(true);
    expect(wordExact(matrix[3])).toBe(false);
  });

  it("says what differs and offers the engine's fixes", () => {
    const issues: CompatibilityIssue[] = [
      { code: "highlight", message: "A highlight colour Word lacks.", location: { block: 0 }, fix: { type: "nearestHighlight", color: "yellow" } },
      { code: "odtObject", message: "A drawing Word can’t show." },
    ];
    expect(compatibilityLine([])).toBe("Everything here opens the same in Word.");
    expect(compatibilityLine(issues)).toBe("2 things will look different in Word");
    expect(fixableCount(issues)).toBe(1);
    expect(fixLabel(issues[0])).toBe("Use Word’s Colour");
    expect(fixLabel(issues[1])).toBeNull();
    expect(fixLabel({ code: "font", message: "", fix: { type: "replaceFont", from: "Liberation Serif", to: "Times New Roman" } })).toBe("Use Times New Roman");
    expect(switchQuestion([])).toBeNull();
    expect(switchQuestion(issues)).toBe("2 things in this document aren’t Word-exact. Fix All changes the 1 Letters can fix; Show lists the rest.");
    expect(switchQuestion([issues[0]])).toBe("1 thing in this document isn’t Word-exact. Fix All changes it so Word shows exactly what you see. Undo takes it back.");
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

import { describe, expect, it } from "vitest";
import { commandForKey, type KeyPress } from "../commands";
import { documentCss, fontStack, lineHeight, paragraphCss, styleClass } from "./css";
import type { DocumentMeta } from "./model";
import { emptyModel } from "./model";
import { preservedKind, preservedLabel } from "./preserved";
import { recentDetails, routeForPath, unsupportedMessage } from "./routing";
import { addTabStop, markerDescription, markerPositions, moveMarker, moveTabStop, nudge, removeTabStop, snap, ticks, type RulerFrame } from "./ruler";
import { WRITER_SHORTCUTS, writerCommandForKey } from "./shortcuts";
import { formatLength, lengthToTwips, twipsToPx, unitForLocale } from "./units";

const letter: RulerFrame = { pageWidth: 12240, marginLeft: 1440, marginRight: 1440 };

describe("the ruler", () => {
  it("places the three markers from the paragraph's indents", () => {
    expect(markerPositions(letter, {})).toEqual({ firstLine: 1440, left: 1440, right: 10800 });
    expect(markerPositions(letter, { left: 720, hanging: 720 })).toEqual({ firstLine: 1440, left: 2160, right: 10800 });
    expect(markerPositions(letter, { firstLine: 720, right: 360 })).toEqual({ firstLine: 2160, left: 1440, right: 10440 });
  });

  it("dragging First Line makes a first-line indent; dragging Left makes a hanging one", () => {
    expect(moveMarker(letter, {}, "firstLine", 2160)).toEqual({ left: 0, firstLine: 720 });
    expect(moveMarker(letter, {}, "left", 2160)).toEqual({ left: 720, hanging: 720 });
    expect(moveMarker(letter, {}, "right", 10080)).toEqual({ left: 0, firstLine: 0, right: 720 });
  });

  it("markers stay on the page and leave room for text", () => {
    expect(moveMarker(letter, {}, "firstLine", -5000).firstLine).toBeUndefined();
    expect(moveMarker(letter, {}, "firstLine", -5000)).toEqual({ left: 0, hanging: 1440 });
    expect(markerPositions(letter, moveMarker(letter, {}, "left", 20000)).left).toBe(10800 - 720);
  });

  it("snaps and nudges in the person's units", () => {
    expect(snap(733, "in")).toBe(720);
    expect(snap(580, "cm")).toBe(567);
    expect(nudge(letter, {}, "firstLine", 1, "in", false)).toEqual({ left: 0, firstLine: 180 });
    expect(nudge(letter, {}, "firstLine", 1, "in", true)).toEqual({ left: 0, firstLine: 720 });
  });

  it("tab stops can be added, moved and removed", () => {
    let tabs = addTabStop([], 2880);
    tabs = addTabStop(tabs, 1440);
    expect(tabs.map((stop) => stop.position)).toEqual([1440, 2880]);
    tabs = moveTabStop(tabs, 2880, 4320);
    expect(tabs.map((stop) => stop.position)).toEqual([1440, 4320]);
    expect(removeTabStop(tabs, 1440)).toEqual([{ position: 4320, kind: "left" }]);
  });

  it("ticks are numbered from the left margin", () => {
    const inches = ticks(letter, "in");
    expect(inches.filter((tick) => tick.kind === "number").map((tick) => tick.label)).toEqual(["1", "", "1", "2", "3", "4", "5", "6", "7"]);
    expect(ticks(letter, "cm").filter((tick) => tick.kind === "number").length).toBeGreaterThan(18);
  });

  it("explains each marker in plain words", () => {
    const format = (twips: number) => formatLength(twips, "in");
    expect(markerDescription("firstLine", letter, { firstLine: 720 }, format)).toContain("indented 0.5 in");
    expect(markerDescription("firstLine", letter, { left: 720, hanging: 720 }, format)).toContain("reference list");
    expect(markerDescription("right", letter, {}, format)).toContain("0 in before the right margin");
  });
});

describe("units", () => {
  it("inches in the US, centimetres elsewhere", () => {
    expect(unitForLocale("en-US")).toBe("in");
    expect(unitForLocale("en-GB")).toBe("cm");
    expect(unitForLocale("de")).toBe("cm");
    expect(unitForLocale("en")).toBe("in");
    expect(formatLength(720, "in")).toBe("0.5 in");
    expect(formatLength(567, "cm")).toBe("1 cm");
    expect(lengthToTwips(2.54, "cm")).toBe(1440);
    expect(twipsToPx(1440)).toBe(96);
  });
});

describe("how the document looks", () => {
  it("paragraph formatting becomes css", () => {
    expect(paragraphCss({ alignment: "center", indent: { left: 720, hanging: 360 }, spacing: { after: 240, line: { rule: "auto", value: 480 } } })).toBe(
      "text-align:center;margin-left:48px;text-indent:-24px;margin-bottom:16px;line-height:2.3",
    );
    expect(lineHeight("exact", 360)).toBe("24px");
    expect(fontStack("Times New Roman")).toContain("serif");
  });

  it("one rule per paragraph style, including the default", () => {
    const meta = emptyModel() as unknown as DocumentMeta;
    meta.styles.styles.push({ id: "Heading 1", name: "heading 1", kind: "paragraph", basedOn: "Normal", run: { bold: true, size: 32 } });
    const css = documentCss(meta);
    expect(css).toContain(`.w-doc .${styleClass("Heading 1")}{font-size:16pt;font-weight:700}`);
    expect(css).toContain(".w-doc .s-default{");
    expect(styleClass("Heading 1")).toBe("s-Heading_201");
  });

  it("preserved content is named", () => {
    expect(preservedKind("<w:drawing><c:chart r:id=\"rId5\"/></w:drawing>")).toBe("Chart");
    expect(preservedLabel("<wps:txbx/>")).toBe("Text box — kept as it is");
    expect(preservedKind("<w:unknown/>")).toBe("Item");
  });
});

describe("opening files", () => {
  it("routes by type", () => {
    expect(routeForPath("C:\\a\\essay.PDF")).toBe("viewer");
    expect(routeForPath("/a/essay.docx")).toBe("writer");
    expect(routeForPath("notes.odt")).toBe("writer");
    expect(routeForPath("sheet.xlsx")).toBe("unsupported");
    expect(unsupportedMessage("C:\\old.doc")).toContain("save it as .docx");
    expect(recentDetails("a.docx", 0, (n) => `${n} pages`)).toBe("Word document");
    expect(recentDetails("a.pdf", 3, (n) => `${n} pages`)).toBe("3 pages");
  });
});

describe("writer shortcuts", () => {
  const press = (code: string, extra: Partial<KeyPress> = {}): KeyPress => ({
    key: code.replace(/^(Key|Digit)/, "").toLowerCase(),
    code,
    ctrlKey: true,
    metaKey: false,
    altKey: false,
    shiftKey: false,
    ...extra,
  });

  it("standard word-processor keys", () => {
    expect(writerCommandForKey(press("KeyB"))).toBe("format-bold");
    expect(writerCommandForKey(press("KeyB", { ctrlKey: false, metaKey: true }))).toBe("format-bold");
    expect(writerCommandForKey(press("Digit1", { altKey: true }))).toBe("style-heading-1");
    expect(writerCommandForKey(press("Digit8", { shiftKey: true }))).toBe("list-bullets");
    expect(writerCommandForKey(press("KeyB", { ctrlKey: false }))).toBeNull();
    // ctrl + alt reported as AltGr: a digit still makes a heading, a typed ³ does not
    expect(writerCommandForKey(press("Digit2", { altKey: true, altGraph: true, typing: true }))).toBe("style-heading-2");
    expect(writerCommandForKey(press("Digit3", { key: "³", altKey: true, altGraph: true, typing: true }))).toBeNull();
  });

  it("no two writer commands share a key", () => {
    const keys = WRITER_SHORTCUTS.map((shortcut) => `${shortcut.code}/${!!shortcut.shift}/${!!shortcut.alt}`);
    expect(new Set(keys).size).toBe(keys.length);
  });

  it("never takes a key every screen needs (open, save, undo, find, zoom, new window)", () => {
    const shared = new Set(["open", "new-window", "save", "save-as", "undo", "redo", "find", "zoom-in", "zoom-out", "fit-width", "toggle-inspector"]);
    for (const shortcut of WRITER_SHORTCUTS) {
      const app = commandForKey(press(shortcut.code, { shiftKey: !!shortcut.shift, altKey: !!shortcut.alt }));
      expect(app === null || !shared.has(app), `${shortcut.command} vs ${app}`).toBe(true);
    }
  });
});

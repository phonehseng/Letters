import { describe, expect, it } from "vitest";
import {
  addedAnnouncement,
  annotationAt,
  fillOf,
  frameBehaviour,
  hits,
  kindLabel,
  popoverSpot,
  strokeWidthOf,
  styleControls,
} from "./annotations";
import type { AnnotationDetails, AnnotationInfo } from "./annotateApi";
import { cssFont, flattenEffect, fontParts, markEffect, pagesPhrase, placeEffect, standardFont } from "./annotateEffects";
import { commentByline, commentsSummary, commentText, commentThreads } from "./comments";
import { colorName, FILL_SWATCHES, HIGHLIGHT_SWATCHES, HIGHLIGHTER, INK_SWATCHES, NOTE_SWATCHES, rgbCss, sameColor } from "./palette";

function annotation(fields: Partial<AnnotationInfo> & { details?: AnnotationDetails }): AnnotationInfo {
  return {
    id: "a",
    page: 0,
    kind: "square",
    subtype: "Square",
    rect: { x: 100, y: 100, width: 100, height: 50 },
    color: { r: 0, g: 0, b: 0 },
    opacity: 1,
    contents: null,
    author: "Sam",
    modified: null,
    created: null,
    replyTo: null,
    hidden: false,
    editable: true,
    details: { type: "shape", strokeWidth: 2, dash: [], fill: null },
    ...fields,
  };
}

describe("the palette", () => {
  const accent = { r: 0, g: 136, b: 255 };
  const all = [...HIGHLIGHT_SWATCHES, ...INK_SWATCHES, ...FILL_SWATCHES, ...NOTE_SWATCHES];

  it("never offers the accent blue, which means selected", () => {
    for (const swatch of all) expect(sameColor(swatch.color, accent), swatch.name).toBe(false);
  });

  it("names every swatch once per palette", () => {
    for (const palette of [HIGHLIGHT_SWATCHES, INK_SWATCHES, FILL_SWATCHES, NOTE_SWATCHES]) {
      const names = palette.map((swatch) => swatch.name);
      expect(new Set(names).size).toBe(names.length);
    }
  });

  it("keeps highlights light enough to read black text through", () => {
    for (const { color } of HIGHLIGHT_SWATCHES) {
      const lightness = 0.2126 * color.r + 0.7152 * color.g + 0.0722 * color.b;
      expect(lightness).toBeGreaterThan(150);
    }
  });

  it("says a colour's name, or that it came from elsewhere", () => {
    expect(colorName(HIGHLIGHT_SWATCHES[0].color, HIGHLIGHT_SWATCHES)).toBe("Yellow");
    expect(colorName({ r: 1, g: 2, b: 3 }, HIGHLIGHT_SWATCHES)).toBe("Custom colour");
    expect(colorName(null, FILL_SWATCHES)).toBe("None");
  });

  it("writes css colours and has a see-through highlighter", () => {
    expect(rgbCss({ r: 1, g: 2, b: 3 })).toBe("rgb(1 2 3)");
    expect(rgbCss({ r: 1, g: 2, b: 3 }, 0.5)).toBe("rgb(1 2 3 / 0.5)");
    expect(HIGHLIGHTER.opacity).toBeLessThan(1);
    expect(HIGHLIGHTER.width).toBeGreaterThan(8);
  });
});

describe("picking an annotation with a click", () => {
  const box = annotation({ id: "box" });
  const line = annotation({
    id: "line",
    kind: "line",
    rect: { x: 0, y: 0, width: 200, height: 200 },
    details: { type: "line", start: { x: 0, y: 0 }, end: { x: 200, y: 200 }, startEnding: "none", endEnding: "openArrow", strokeWidth: 2, dash: [], fill: null },
  });
  const ink = annotation({
    id: "ink",
    kind: "ink",
    rect: { x: 300, y: 300, width: 100, height: 100 },
    details: { type: "ink", strokes: [[{ x: 300, y: 300 }, { x: 400, y: 300 }]], width: 2 },
  });

  it("picks boxes inside their rectangle, and lines and strokes only near them", () => {
    expect(hits(box, { x: 150, y: 120 })).toBe(true);
    expect(hits(line, { x: 100, y: 102 })).toBe(true);
    expect(hits(line, { x: 150, y: 20 })).toBe(false);
    expect(hits(ink, { x: 350, y: 302 })).toBe(true);
    expect(hits(ink, { x: 350, y: 380 })).toBe(false);
  });

  it("picks the one on top, and skips replies and hidden ones", () => {
    const below = annotation({ id: "below" });
    const above = annotation({ id: "above" });
    const reply = annotation({ id: "reply", replyTo: "above" });
    const hidden = annotation({ id: "hidden", hidden: true });
    expect(annotationAt([below, above, reply, hidden], 0, { x: 150, y: 120 })?.id).toBe("above");
    expect(annotationAt([below], 1, { x: 150, y: 120 })).toBeNull();
  });
});

describe("what a selected annotation allows", () => {
  it("moves and resizes shapes, keeps pictures in proportion, and only moves notes", () => {
    expect(frameBehaviour(annotation({}))).toEqual({ movable: true, resizable: true, keepAspect: false });
    expect(frameBehaviour(annotation({ kind: "stamp", details: { type: "stamp", isPicture: true } })).keepAspect).toBe(true);
    expect(frameBehaviour(annotation({ kind: "note", details: { type: "note", icon: "comment", open: false, popup: null } }))).toEqual({
      movable: true,
      resizable: false,
      keepAspect: false,
    });
  });

  it("leaves marks on text, and annotations Letters can't change, where they are", () => {
    const highlight = annotation({ kind: "highlight", details: { type: "textMarkup", quads: [] } });
    expect(frameBehaviour(highlight).movable).toBe(false);
    expect(frameBehaviour(annotation({ editable: false })).resizable).toBe(false);
    expect(styleControls(annotation({ editable: false })).color).toBe("none");
  });

  it("offers the style controls that fit each kind", () => {
    expect(styleControls(annotation({}))).toEqual({ color: "ink", strokeWidth: true, fill: true, opacity: true, text: false });
    expect(styleControls(annotation({ kind: "highlight", details: { type: "textMarkup", quads: [] } })).color).toBe("highlight");
    expect(styleControls(annotation({ kind: "underline", details: { type: "textMarkup", quads: [] } })).color).toBe("ink");
    expect(styleControls(annotation({ kind: "ink", details: { type: "ink", strokes: [], width: 3 } })).fill).toBe(false);
    expect(strokeWidthOf(annotation({ kind: "ink", details: { type: "ink", strokes: [], width: 3 } }))).toBe(3);
    expect(fillOf(annotation({ details: { type: "shape", strokeWidth: 1, dash: [], fill: { r: 1, g: 1, b: 1 } } }))).toEqual({ r: 1, g: 1, b: 1 });
  });

  it("names kinds, telling arrows from lines", () => {
    const arrow = annotation({
      kind: "line",
      details: { type: "line", start: { x: 0, y: 0 }, end: { x: 1, y: 1 }, startEnding: "none", endEnding: "openArrow", strokeWidth: 1, dash: [], fill: null },
    });
    expect(kindLabel(arrow)).toBe("Arrow");
    expect(kindLabel(annotation({ kind: "textBox", details: { type: "none" } }))).toBe("Text box");
    expect(addedAnnouncement("Highlight", 2)).toBe("Highlight added to page 3.");
  });

  it("puts a note's pop-up beside it, inside the page", () => {
    const page = { width: 612, height: 792 };
    expect(popoverSpot({ x: 100, y: 100, width: 20, height: 20 }, page, 200)).toEqual({ x: 126, y: 100 });
    expect(popoverSpot({ x: 580, y: 100, width: 20, height: 20 }, page, 200)).toEqual({ x: 374, y: 100 });
  });
});

describe("comment threads", () => {
  const note = (id: string, page: number, y: number, extra: Partial<AnnotationInfo> = {}) =>
    annotation({ id, page, kind: "note", rect: { x: 10, y, width: 20, height: 20 }, details: { type: "note", icon: "comment", open: false, popup: null }, contents: id, ...extra });

  it("groups replies under their comment, oldest first, and replies to replies too", () => {
    const root = note("root", 0, 100);
    const late = note("late", 0, 100, { replyTo: "root", created: "2026-11-11T12:00:00Z" });
    const early = note("early", 0, 100, { replyTo: "root", created: "2026-11-11T10:00:00Z" });
    const nested = note("nested", 0, 100, { replyTo: "late", created: "2026-11-11T13:00:00Z" });
    const threads = commentThreads([root, late, early, nested]);
    expect(threads).toHaveLength(1);
    expect(threads[0].replies.map((reply) => reply.id)).toEqual(["early", "late", "nested"]);
  });

  it("lists comments in reading order and leaves out marks without a comment", () => {
    const plainHighlight = annotation({ id: "plain", kind: "highlight", details: { type: "textMarkup", quads: [] } });
    const remark = annotation({ id: "remark", kind: "highlight", contents: "Key point", rect: { x: 0, y: 50, width: 5, height: 5 }, details: { type: "textMarkup", quads: [] } });
    const threads = commentThreads([note("second", 1, 10), note("lower", 0, 300), plainHighlight, remark]);
    expect(threads.map((thread) => thread.root.id)).toEqual(["remark", "lower", "second"]);
  });

  it("writes the byline, the row's text and the summary", () => {
    const now = Date.parse("2026-11-11T12:05:00Z") / 1000;
    const row = note("row", 2, 0, { modified: "2026-11-11T12:00:00Z", contents: "  " });
    expect(commentByline(row, now)).toBe("Sam · 5 minutes ago · page 3");
    expect(commentByline({ ...row, author: null }, now, false)).toBe("Unknown author · 5 minutes ago");
    expect(commentText(row)).toBe("Note without a comment");
    expect(commentsSummary([])).toBe("No comments yet");
    const threads = commentThreads([note("one", 0, 0), note("reply", 0, 0, { replyTo: "one" })]);
    expect(commentsSummary(threads)).toBe("1 comment, 1 reply");
  });
});

describe("the sentences under the tools", () => {
  it("says what a mark will do to the selection, or how to use the tool", () => {
    expect(markEffect("highlight", [2], false)).toBe("Highlights the selected text on page 3. Undo with Ctrl Z.");
    expect(markEffect("strikeOut", [1, 2, 3], false)).toContain("pages 2 to 4");
    expect(markEffect("underline", null, true)).toBe("Drag across text on the page to underline it. Press Escape when you are done.");
    expect(markEffect("highlight", null, false)).toContain("click Highlight");
    expect(pagesPhrase([])).toBe("this page");
  });

  it("explains placing tools and flattening", () => {
    expect(placeEffect("a note", "Click where the note should go.", true)).toBe("Click where the note should go. Press Escape when you are done.");
    expect(placeEffect("a note", "", false)).toBe("Adds a note to your working copy. Undo with Ctrl Z.");
    expect(flattenEffect(0, "all")).toContain("no comments");
    expect(flattenEffect(1, [0])).toContain("1 comment or mark on page 1");
    expect(flattenEffect(5, "all")).toContain("Undo still works until you close the document");
  });

  it("puts fonts together and apart", () => {
    expect(standardFont("times", true, true)).toBe("timesBoldItalic");
    expect(standardFont("helvetica", false, false)).toBe("helvetica");
    expect(fontParts("courierOblique")).toEqual({ family: "courier", bold: false, italic: true });
    expect(cssFont("helveticaBold")).toEqual({ family: "Helvetica, Arial, sans-serif", weight: 700, style: "normal" });
  });
});

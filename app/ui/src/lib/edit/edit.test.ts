// tests for the Edit tab's pure parts: finding paragraphs and pictures,
// the editor's place on the baseline, typing turned into the smallest
// change, the font sentences and the editor's fonts. the page objects are
// built by hand in the shape core/edit answers.
import { describe, expect, it } from "vitest";
import { blockAt, caretIndexFor, imageAt, isUpright, lineAt, matchingBlock, matchingImage, visibleBlocks } from "./blocks";
import type { FontInfo, ImageObject, PageObjects, TextBlock, TextLine, TextRun } from "./editApi";
import { baselineOffset, editorBox, lineSpacing, newTextBox } from "./editorGeometry";
import { editedAnnouncement, noticeSentence, overflowSentence, plainFontName, quotedList, typingHint } from "./editSentences";
import { rangeInOneLine } from "./editSession.svelte";
import { fontStack, hexToRgb, nearestStandard, rgbToHex } from "./cssFonts";
import { clearsBlock, editorText, lineChange, missingCharacters, planTextChange, sharedEnds } from "./textDiff";

const font: FontInfo = {
  id: "font:7-0",
  name: "Arial-BoldMT",
  family: "sansSerif",
  bold: true,
  italic: false,
  embedded: true,
  subset: true,
  kind: "trueType",
  editableChars: "Thequickbrownfox ",
};

function run(text: string, x: number, y: number, size = 12, invisible = false): TextRun {
  return {
    id: `run-${text}`,
    text,
    font,
    size,
    color: { r: 0, g: 0, b: 0 },
    charBoxes: [...text].map((_, index) => ({ x: x + index * 6, y, width: 6, height: size })),
    invisible,
  };
}

function line(id: string, text: string, x: number, baseline: number, size = 12, invisible = false): TextLine {
  const width = text.length * 6;
  return {
    id,
    rect: { x, y: baseline - size * 0.8, width, height: size },
    text,
    baseline: [
      { x, y: baseline },
      { x: x + width, y: baseline },
    ],
    runs: [run(text, x, baseline - size * 0.8, size, invisible)],
  };
}

function block(id: string, lines: TextLine[]): TextBlock {
  const left = Math.min(...lines.map((each) => each.rect.x));
  const top = Math.min(...lines.map((each) => each.rect.y));
  const right = Math.max(...lines.map((each) => each.rect.x + each.rect.width));
  const bottom = Math.max(...lines.map((each) => each.rect.y + each.rect.height));
  return { id, rect: { x: left, y: top, width: right - left, height: bottom - top }, text: lines.map((each) => each.text).join("\n"), lines, rotation: 0, alignment: "left" };
}

function image(id: string, x: number, y: number, width: number, height: number): ImageObject {
  return {
    id,
    rect: { x, y, width, height },
    quad: [
      { x, y },
      { x: x + width, y },
      { x, y: y + height },
      { x: x + width, y: y + height },
    ],
    pixelWidth: 100,
    pixelHeight: 100,
    colorSpace: "DeviceRGB",
    bitsPerComponent: 8,
    hasAlpha: false,
    inline: false,
    filter: "DCTDecode",
  };
}

const paragraph = block("b1", [line("l1", "The quick brown", 72, 100), line("l2", "fox jumps over", 72, 114.4)]);
const heading = block("b2", [line("l3", "Title", 72, 60, 24)]);
const page: PageObjects = { page: 0, width: 612, height: 792, textBlocks: [paragraph, heading], images: [image("i1", 300, 300, 100, 80), image("i2", 350, 320, 40, 40)] };

describe("finding what is under the pointer", () => {
  it("picks the block under a point, smallest first", () => {
    expect(blockAt(page, { x: 80, y: 95 })?.id).toBe("b1");
    expect(blockAt(page, { x: 80, y: 45 })?.id).toBe("b2");
    expect(blockAt(page, { x: 500, y: 500 })).toBeNull();
  });

  it("leaves recognized (invisible) text to the Scan tab", () => {
    const scanned: PageObjects = { ...page, textBlocks: [block("ocr", [line("o1", "Scanned words", 72, 200, 12, true)])] };
    expect(visibleBlocks(scanned)).toHaveLength(0);
    expect(blockAt(scanned, { x: 80, y: 195 })).toBeNull();
  });

  it("finds the line by height and the caret across it", () => {
    expect(lineAt(paragraph, { x: 80, y: 112 })?.id).toBe("l2");
    // half way along the second line: 7 of its 14 characters, after the first line and its new line
    expect(caretIndexFor(paragraph, { x: 72 + 42, y: 112 })).toBe(16 + 7);
    expect(caretIndexFor(paragraph, { x: 0, y: 95 })).toBe(0);
  });

  it("picks the picture drawn on top", () => {
    expect(imageAt(page, { x: 360, y: 330 })?.id).toBe("i2");
    expect(imageAt(page, { x: 310, y: 310 })?.id).toBe("i1");
  });

  it("finds a block and a picture again after the page changed", () => {
    const moved = { ...page, textBlocks: [{ ...paragraph, id: "fresh", rect: { ...paragraph.rect, width: paragraph.rect.width + 30 } }, heading] };
    expect(matchingBlock(moved, paragraph.rect)?.id).toBe("fresh");
    expect(matchingImage(page, { x: 301, y: 301, width: 99, height: 80 })?.id).toBe("i1");
    expect(matchingBlock(page, { x: 500, y: 700, width: 10, height: 10 })).toBeNull();
  });

  it("knows an upright picture from a turned one", () => {
    expect(isUpright(image("u", 0, 0, 10, 10))).toBe(true);
    const turned = { ...image("t", 0, 0, 10, 10), quad: [{ x: 5, y: 0 }, { x: 10, y: 5 }, { x: 0, y: 5 }, { x: 5, y: 10 }] as ImageObject["quad"] };
    expect(isUpright(turned)).toBe(false);
  });
});

describe("the editor over the text", () => {
  const metrics = { ascent: 0.9, descent: 0.2 };

  it("measures line spacing from the baselines", () => {
    expect(lineSpacing(paragraph)).toBeCloseTo(14.4);
    expect(lineSpacing(heading)).toBeCloseTo(24 * 1.2);
  });

  it("puts the first baseline exactly on the page's baseline, at any zoom", () => {
    for (const scale of [1, 1.5, 2.25]) {
      const box = editorBox(paragraph, { left: 40, top: 500, scale }, metrics);
      const baseline = box.top + baselineOffset(box.lineHeight, box.fontSize, metrics);
      expect(baseline).toBeCloseTo(500 + 100 * scale);
      expect(box.left).toBeCloseTo(40 + 72 * scale);
      expect(box.fontSize).toBeCloseTo(12 * scale);
      expect(box.height).toBeCloseTo(2 * 14.4 * scale);
    }
  });

  it("grows a line at a time as lines are typed", () => {
    const box = editorBox(paragraph, { left: 0, top: 0, scale: 1 }, metrics, 3);
    expect(box.height).toBeCloseTo(3 * 14.4);
  });

  it("starts new text where the engine will put it", () => {
    const place = newTextBox({ x: 100, y: 200 }, 12, { left: 0, top: 0, scale: 2 }, metrics);
    const baseline = place.top + baselineOffset(place.lineHeight, place.fontSize, metrics);
    // the engine's first baseline is the top plus the font's ascent
    expect(baseline).toBeCloseTo(2 * (200 + 0.9 * 12));
  });
});

describe("turning typing into the smallest change", () => {
  it("finds what two strings share at each end", () => {
    expect(sharedEnds("colour", "color")).toEqual({ prefix: 4, suffix: 1 });
    expect(sharedEnds("aaa", "aaaa")).toEqual({ prefix: 3, suffix: 0 });
  });

  it("makes an insert, a delete or a rewrite of one line", () => {
    expect(lineChange("l", "The fox", "The red fox")).toEqual({ kind: "insert", lineId: "l", index: 4, text: "red " });
    expect(lineChange("l", "The red fox", "The fox")).toEqual({ kind: "delete", lineId: "l", start: 4, end: 8 });
    expect(lineChange("l", "The fox", "A cat")).toEqual({ kind: "replaceLine", lineId: "l", text: "A cat" });
    expect(lineChange("l", "same", "same")).toEqual({ kind: "none" });
  });

  it("keeps a change in one line to that line", () => {
    expect(editorText(paragraph)).toBe("The quick brown\nfox jumps over");
    expect(planTextChange(paragraph, "The quick brown\nfox leaps over")).toEqual({ kind: "replaceLine", lineId: "l2", text: "fox leaps over" });
    expect(planTextChange(paragraph, "The quick, brown\nfox jumps over")).toEqual({ kind: "insert", lineId: "l1", index: 9, text: "," });
    expect(planTextChange(paragraph, editorText(paragraph))).toEqual({ kind: "none" });
  });

  it("re-sets the paragraph when lines were added or several changed", () => {
    expect(planTextChange(paragraph, "A slow red\ncat sat\nquietly")).toEqual({ kind: "replaceBlock", blockId: "b1", text: "A slow red cat sat quietly" });
    expect(planTextChange(paragraph, "One\nTwo")).toEqual({ kind: "replaceBlock", blockId: "b1", text: "One Two" });
    expect(clearsBlock(planTextChange(paragraph, "\n"))).toBe(true);
  });

  it("names letters the font can't draw, once each", () => {
    expect(missingCharacters("The café é", "The quick", font.editableChars)).toBe("aé");
    expect(missingCharacters("The quick", "The quick", font.editableChars)).toBe("");
  });

  it("finds a selection inside one line of the paragraph", () => {
    expect(rangeInOneLine(paragraph, [16, 19])).toEqual({ lineId: "l2", range: [0, 3] });
    expect(rangeInOneLine(paragraph, [4, 9])).toEqual({ lineId: "l1", range: [4, 9] });
    // across the line break, or nothing selected: the whole paragraph
    expect(rangeInOneLine(paragraph, [10, 20])).toBeNull();
    expect(rangeInOneLine(paragraph, [5, 5])).toBeNull();
  });
});

describe("the font notice sentences", () => {
  it("says which font drew the missing letters", () => {
    expect(
      noticeSentence({ kind: "installedFont", originalFont: "Arial-BoldMT", usedFont: "Arial", characters: "é", message: "" }),
    ).toBe("The original font can’t draw “é”, so Arial from this computer was used for it.");
    expect(
      noticeSentence({ kind: "standardFont", originalFont: "Foo", usedFont: "Helvetica", characters: "éß", message: "" }),
    ).toBe("The original font can’t draw “é” and “ß”, so Helvetica, a font every PDF reader has, was used for them.");
    const refused = "The original font can’t draw “é”, and the copy of Foo on this computer doesn’t allow being copied into a PDF, so Helvetica was used for the new text.";
    expect(noticeSentence({ kind: "standardFont", originalFont: "Foo", usedFont: "Helvetica", characters: "é", message: refused })).toBe(refused);
  });

  it("lists characters and plain font names", () => {
    expect(quotedList("a")).toBe("“a”");
    expect(quotedList("abc")).toBe("“a”, “b” and “c”");
    expect(plainFontName("ABCDEF+TimesNewRomanPSMT")).toBe("Times New Roman");
    expect(plainFontName("Arial-BoldMT")).toBe("Arial");
  });

  it("explains an overflow and a letter that will change font", () => {
    expect(overflowSentence({ linesBefore: 2, linesAfter: 4, extraHeight: 28.8 })).toBe(
      "The new text needs 2 more lines than the paragraph had, so it now reaches 29 pt lower and may cover what is below.",
    );
    expect(typingHint("é", font)).toContain("“é” isn’t in this copy of Arial");
    expect(typingHint("", font)).toBe("");
    expect(editedAnnouncement(2, [])).toBe("Text changed on page 3.");
  });
});

describe("the editor's fonts", () => {
  it("stacks the loaded face, the font by name and its kind", () => {
    expect(fontStack(font, "Letters page font x")).toBe('"Letters page font x", "Arial", Arial, Helvetica, "Segoe UI", sans-serif');
    expect(fontStack({ ...font, family: "serif" }, null)).toBe('"Arial", "Times New Roman", Times, serif');
  });

  it("finds the nearest standard font and converts colours", () => {
    expect(nearestStandard(font)).toBe("helveticaBold");
    expect(nearestStandard({ family: "serif", bold: false, italic: true })).toBe("timesItalic");
    expect(rgbToHex({ r: 51, g: 102, b: 153 })).toBe("#336699");
    expect(hexToRgb("#336699")).toEqual({ r: 51, g: 102, b: 153 });
  });
});

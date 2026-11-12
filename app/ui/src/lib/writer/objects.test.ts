// where free objects sit, snapping, text wrap, cropping and turning: the pure maths behind the object frame
import { describe, expect, it } from "vitest";
import { angleFromPointer, cropDrag, cropPlacement, isUncropped, normalAngle, rotatedBounds, snapAngle, uncroppedSize } from "./crop";
import { guideOnScreen, resizedBox } from "./frameMaths";
import { anchorBlock, freeObjects, objectRect, pageAt, pageX, toPageTwips, zIndexFor, type FreeObject, type SurfaceLayout } from "./objectGeometry";
import { snap, snapThreshold } from "./snapping";
import { exclusionFor, exclusionStyle, hostBlock, pushesText, WRAP_DISTANCE, type WrapLayout } from "./wrap";
import { para, stateWith } from "./testDocs";

// letter paper, one inch margins, two pages on screen
const layout: SurfaceLayout = {
  pageWidth: 12240,
  pageHeight: 15840,
  margins: { top: 1440, right: 1440, bottom: 1440, left: 1440 },
  pageEdges: [-96, 1000],
  blockPages: [0, 0, 1],
  blockTops: [0, 40, 1100],
};

const object = (extra: Partial<FreeObject>): FreeObject => ({
  kind: "image",
  pos: 5,
  block: 1,
  horizontal: { relativeTo: "page", offset: 2880 },
  vertical: { relativeTo: "page", offset: 4320 },
  width: 1440,
  height: 720,
  wrap: "square",
  zOrder: 1,
  rotation: 0,
  ...extra,
});

describe("where free objects sit", () => {
  it("relative to the page, the margins or the paragraph, as Word measures them", () => {
    expect(objectRect(object({}), layout)).toEqual({ left: 2880 / 15 - 96, top: -96 + 288, width: 96, height: 48 });
    expect(objectRect(object({ horizontal: { relativeTo: "margin", offset: 0 }, vertical: { relativeTo: "margin", offset: 0 } }), layout)).toMatchObject({ left: 0, top: 0 });
    expect(objectRect(object({ horizontal: { relativeTo: "column", offset: 150 }, vertical: { relativeTo: "paragraph", offset: 300 } }), layout)).toMatchObject({ left: 10, top: 40 + 20 });
    // on the second page, its own page's edge counts
    expect(objectRect(object({ block: 2 }), layout).top).toBe(1000 + 288);
  });

  it("aligned positions use the page or the text column", () => {
    expect(pageX({ relativeTo: "page", align: "center" }, 1440, layout)).toBe((12240 - 1440) / 2);
    expect(pageX({ relativeTo: "margin", align: "right" }, 1440, layout)).toBe(12240 - 1440 - 1440);
    expect(pageX({ relativeTo: "column", align: "left" }, 1440, layout)).toBe(1440);
  });

  it("page coordinates and the screen convert both ways without drift (no jump)", () => {
    for (const [x, y] of [[0, 0], [2880, 4321], [11000, 15000]]) {
      const rect = objectRect(object({ horizontal: { relativeTo: "page", offset: x }, vertical: { relativeTo: "page", offset: y } }), layout);
      expect(toPageTwips(rect.left, rect.top, 0, layout)).toEqual({ x, y });
    }
    expect(pageAt(500, layout)).toBe(0);
    expect(pageAt(1000, layout)).toBe(1);
  });

  it("a text block is anchored at the next block in the flow", () => {
    expect(anchorBlock(1, [0, Number.NaN, 30])).toBe(2);
    expect(anchorBlock(2, [0, 10, Number.NaN])).toBe(1);
  });

  it("behind-text objects go under the words, others above in stacking order", () => {
    expect(zIndexFor({ wrap: "behindText", zOrder: 2 })).toBeLessThan(0);
    expect(zIndexFor({ wrap: "square", zOrder: 5 })).toBeGreaterThan(zIndexFor({ wrap: "square", zOrder: 2 }));
    expect(Number.isInteger(zIndexFor({ wrap: "behindText", zOrder: 3.5 }))).toBe(true);
  });

  it("lists floating pictures and text blocks with their places", () => {
    const state = stateWith([
      { type: "paragraph", content: [{ type: "text", text: "a" }, { type: "image", mediaId: "a.png", width: 100, height: 100, placement: { type: "floating", horizontal: { relativeTo: "page", offset: 10 }, vertical: { relativeTo: "page", offset: 20 }, wrap: "topAndBottom" } }] },
      { type: "textBox", horizontal: { relativeTo: "page", offset: 1 }, vertical: { relativeTo: "page", offset: 2 }, width: 300, height: 200, wrap: "square", blocks: [para("x")] },
      { type: "paragraph", content: [{ type: "image", mediaId: "b.png", width: 1, height: 1 }] },
    ]);
    const found = freeObjects(state.doc);
    expect(found.map((each) => [each.kind, each.block, each.wrap])).toEqual([
      ["image", 0, "topAndBottom"],
      ["textBox", 1, "square"],
    ]);
    expect(state.doc.nodeAt(found[0].pos)?.type.name).toBe("image");
    expect(state.doc.nodeAt(found[1].pos)?.type.name).toBe("text_box");
  });
});

describe("snapping, the engine's rules", () => {
  const page = { width: 12240, height: 15840, margins: { top: 1440, right: 1440, bottom: 1440, left: 1440 } };

  it("the engine's own case: a box near the margin snaps to it", () => {
    const snapped = snap(page, [{ x: 3440, y: 1440, width: 1000, height: 500 }], { x: 1460, y: 3000, width: 1000, height: 500 }, 40);
    expect(snapped.rect.x).toBe(1440);
    expect(snapped.guides[0]).toEqual({ vertical: true, position: 1440, kind: "margin" });
  });

  it("centres, other objects' edges, and nothing beyond the threshold", () => {
    const centred = snap(page, [], { x: 6120 - 500 + 20, y: 9000, width: 1000, height: 400 }, 40);
    expect(centred.rect.x).toBe(5620);
    expect(centred.guides[0].kind).toBe("page centre");
    const beside = snap(page, [{ x: 5000, y: 5000, width: 1000, height: 1000 }], { x: 6010, y: 7000, width: 500, height: 500 }, 40);
    expect(beside.rect.x).toBe(6000);
    expect(beside.guides[0].kind).toBe("object edge");
    expect(snap(page, [], { x: 3000, y: 7000, width: 500, height: 500 }, 40).guides).toEqual([]);
    expect(snapThreshold(1)).toBe(90);
    expect(snapThreshold(2)).toBe(45);
  });

  it("a guide in page twips lands on the screen at the zoom", () => {
    const line = guideOnScreen({ vertical: true, position: 1440, kind: "margin" }, { edgeTop: -96, widthTwips: 12240, heightTwips: 15840, marginLeftTwips: 1440 }, { left: 200, top: 100 }, 2);
    expect(line).toEqual({ left: 200, top: 100 - 192, width: 0, height: 2112 });
  });
});

describe("text flowing around objects", () => {
  const wrapLayout: WrapLayout = {
    columnWidth: 624,
    blockTops: [0, 30, Number.NaN, 60, 90],
    holdsText: [true, true, false, true, true],
    pageTextBottom: () => 864,
  };

  it("the float sits in the paragraph just above the object, reaching to its bottom", () => {
    const exclusion = exclusionFor({ left: 0, top: 40, width: 200, height: 100 }, "square", wrapLayout)!;
    expect(exclusion).toEqual({ block: 1, side: "left", dy: 10, width: 200 + WRAP_DISTANCE, height: 110 });
    expect(exclusionStyle(exclusion)).toBe("float:left;width:212.00px;height:110.00px;shape-outside:inset(10.00px 0 0 0)");
  });

  it("text goes to the wider side", () => {
    expect(exclusionFor({ left: 500, top: 0, width: 100, height: 50 }, "square", wrapLayout)).toMatchObject({ side: "right", width: 624 - 488 });
  });

  it("above and below takes the whole column; behind and in front push nothing", () => {
    expect(exclusionFor({ left: 200, top: 61, width: 100, height: 50 }, "topAndBottom", wrapLayout)).toMatchObject({ block: 3, width: 624, dy: 1 });
    expect(exclusionFor({ left: 200, top: 61, width: 100, height: 50 }, "behindText", wrapLayout)).toBeNull();
    expect(pushesText("inFrontOfText")).toBe(false);
  });

  it("stops at the page's bottom margin, and skips objects in the margin", () => {
    expect(exclusionFor({ left: 0, top: 800, width: 100, height: 300 }, "square", wrapLayout)?.height).toBe(864 - 90);
    expect(exclusionFor({ left: -300, top: 40, width: 100, height: 50 }, "square", wrapLayout)).toBeNull();
    expect(hostBlock(-10, wrapLayout)).toBe(0);
    expect(hostBlock(70, wrapLayout)).toBe(3);
  });
});

describe("cropping and turning", () => {
  it("a crop keeps the picture's scale: the shown part gets smaller", () => {
    const start = { width: 200, height: 100 };
    const right = cropDrag(start, { left: 0, top: 0, right: 0, bottom: 0 }, "e", -50, 0);
    expect(right.width).toBe(150);
    expect(right.crop.right).toBe(25000);
    const left = cropDrag(start, { left: 0, top: 0, right: 0, bottom: 0 }, "nw", 20, 10);
    expect(left).toMatchObject({ width: 180, height: 90, shiftX: 20, shiftY: 10 });
    expect(left.crop).toEqual({ left: 10000, top: 10000, right: 0, bottom: 0 });
    expect(uncroppedSize({ width: 150, height: 100 }, right.crop)).toEqual({ width: 200, height: 100 });
    expect(cropPlacement({ width: 180, height: 90 }, left.crop)).toEqual({ left: -20, top: -10, width: 200, height: 100 });
  });

  it("uncropping gives the hidden part back, never past the edge or below a sliver", () => {
    const back = cropDrag({ width: 150, height: 100 }, { left: 0, top: 0, right: 25000, bottom: 0 }, "e", 80, 0);
    expect(back.crop.right).toBe(0);
    expect(back.width).toBe(200);
    expect(cropDrag({ width: 200, height: 100 }, { left: 0, top: 0, right: 0, bottom: 0 }, "e", -500, 0).width).toBeCloseTo(8);
    expect(isUncropped({ left: 0, top: 0, right: 0, bottom: 0 })).toBe(true);
  });

  it("turning snaps to 15 degrees unless Shift frees it", () => {
    const centre = { x: 100, y: 100 };
    expect(angleFromPointer(centre, { x: 100, y: 0 })).toBe(0);
    expect(angleFromPointer(centre, { x: 200, y: 100 })).toBe(90);
    expect(angleFromPointer(centre, { x: 0, y: 100 })).toBe(270);
    expect(snapAngle(52, false)).toBe(45);
    expect(snapAngle(53, false)).toBe(60);
    expect(snapAngle(52.34, true)).toBe(52.3);
    expect(snapAngle(359, false)).toBe(0);
    expect(normalAngle(-90)).toBe(270);
  });

  it("a turned box covers a bigger area for wrapping", () => {
    const turned = rotatedBounds({ left: 0, top: 0, width: 100, height: 50 }, 90);
    expect(turned.width).toBeCloseTo(50);
    expect(turned.height).toBeCloseTo(100);
    expect(turned.left).toBeCloseTo(25);
  });
});

describe("resizing a box", () => {
  it("the opposite corner stays put; from the centre, the centre does", () => {
    const start = { left: 10, top: 10, width: 200, height: 100 };
    expect(resizedBox(start, "nw", -20, 0, { lockAspect: true, fromCentre: false })).toMatchObject({ left: -10, top: 0, width: 220, height: 110 });
    expect(resizedBox(start, "se", 20, 0, { lockAspect: false, fromCentre: false })).toMatchObject({ left: 10, top: 10, width: 220, height: 100 });
    expect(resizedBox(start, "e", 10, 0, { lockAspect: false, fromCentre: true })).toMatchObject({ left: 0, width: 220 });
  });
});

import { describe, expect, it } from "vitest";
import { placePages, PAGE_GAP } from "../layout";
import {
  contentPointFor,
  frameAt,
  frameOf,
  locate,
  locateOnPage,
  pagePointOf,
  rectContains,
  rectOnPage,
  type PageFrame,
} from "./pagePoints";

const letter = { width: 612, height: 792 };
const landscape = { width: 792, height: 612 };

/** frames the way the viewer makes them: page lefts measured from a content centred on `centre`. */
function framesFor(sizes: { width: number; height: number }[], zoom: number, mode: "single" | "twoPages", centre = 1000) {
  const layout = placePages(sizes, zoom, 0, mode);
  return layout.pages.map((page) => frameOf(page, sizes[page.index], centre + page.left));
}

describe("turning pointer positions into page points", () => {
  it("divides by the zoom and measures from the page's corner", () => {
    for (const zoom of [0.5, 1, 1.75, 4]) {
      const [first] = framesFor([letter], zoom, "single");
      expect(first.scale).toBeCloseTo(zoom);
      // the content box sits at (30, 40) on screen
      const point = locate([first], 30 + first.left + 72 * zoom, 40 + first.top + 144 * zoom, { left: 30, top: 40 });
      expect(point?.page).toBe(0);
      expect(point?.x).toBeCloseTo(72);
      expect(point?.y).toBeCloseTo(144);
    }
  });

  it("follows the scroll position, because the content's box moves with it", () => {
    const frames = framesFor([letter, letter, letter], 1, "single");
    const third = frames[2];
    // scrolled down by 1500 pixels: the content's top is now above the window
    const scrolled = { left: 0, top: -1500 };
    const clientY = third.top + 100 - 1500;
    const point = locate(frames, third.left + 10, clientY, scrolled);
    expect(point).toEqual({ page: 2, x: 10, y: 100 });
  });

  it("tells the two pages of a pair apart in two page mode", () => {
    const frames = framesFor([letter, letter, letter], 1, "twoPages");
    const [left, right] = frames;
    expect(left.top).toBe(right.top);
    expect(right.left).toBe(left.left + left.width + PAGE_GAP);
    const onRight = locate(frames, right.left + 50, right.top + 60, { left: 0, top: 0 });
    expect(onRight).toEqual({ page: 1, x: 50, y: 60 });
    // the lone last page keeps the left seat
    expect(frames[2].left).toBe(left.left);
  });

  it("finds nothing in the gap between pages unless the nearest page is asked for", () => {
    const frames = framesFor([letter, letter], 1, "single");
    const gap = { x: frames[0].left + 100, y: frames[0].top + frames[0].height + PAGE_GAP / 2 - 2 };
    expect(frameAt(frames, gap)).toBeNull();
    expect(frameAt(frames, gap, true)?.index).toBe(0);
    expect(locate(frames, gap.x, gap.y, { left: 0, top: 0 }, true)?.y).toBe(792);
  });

  it("keeps a drag on the page it started on, even past its edge", () => {
    const [first] = framesFor([letter], 2, "single");
    expect(locateOnPage(first, first.left - 50, first.top - 20, { left: 0, top: 0 })).toEqual({ page: 0, x: 0, y: 0 });
    const far = locateOnPage(first, first.left + 5000, first.top + 5000, { left: 0, top: 0 });
    expect(far).toEqual({ page: 0, x: 612, y: 792 });
  });

  it("works for pages of different sizes", () => {
    const frames = framesFor([letter, landscape], 1, "single");
    const second = frames[1];
    expect(second.size).toEqual(landscape);
    const point = pagePointOf(second, { x: second.left + 700, y: second.top + 500 });
    expect(point).toEqual({ page: 1, x: 700, y: 500 });
  });

  it("goes back from page points to the screen", () => {
    const [first] = framesFor([letter], 1.5, "single");
    const there = contentPointFor(first, { x: 100, y: 200 });
    expect(pagePointOf(first, there).x).toBeCloseTo(100);
    expect(pagePointOf(first, there).y).toBeCloseTo(200);
    expect(rectOnPage(first, { x: 10, y: 20, width: 30, height: 40 })).toEqual({ x: 15, y: 30, width: 45, height: 60 });
  });

  it("says whether a point is on a rectangle, with some slack", () => {
    const rect = { x: 10, y: 10, width: 20, height: 20 };
    expect(rectContains(rect, { x: 15, y: 15 })).toBe(true);
    expect(rectContains(rect, { x: 32, y: 15 })).toBe(false);
    expect(rectContains(rect, { x: 32, y: 15 }, 3)).toBe(true);
  });

  it("uses the page's own drawn width for its scale", () => {
    const frame: PageFrame = frameOf(
      { index: 0, top: 0, left: 0, width: 306, height: 396, rowTop: 0, rowHeight: 396 },
      letter,
      0,
    );
    expect(frame.scale).toBe(0.5);
  });
});

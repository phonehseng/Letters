import { describe, expect, it } from "vitest";
import {
  anchorAt,
  MAX_ZOOM,
  MIN_ZOOM,
  PAGE_GAP,
  pageAtMiddle,
  pagePixelSize,
  placePages,
  positionOf,
  rowAround,
  rowsOf,
  scaleMilli,
  scrollToCentre,
  scrollToReveal,
  scrollTopForPage,
  tilesForPage,
  type PageSize,
  visiblePages,
  zoomIn,
  zoomOut,
  zoomToFitWidth,
} from "./layout";

const LETTER = { width: 612, height: 792 };
const manyPages = (count: number) => Array.from({ length: count }, () => LETTER);

describe("placePages", () => {
  it("stacks pages with a gap between them", () => {
    const layout = placePages(manyPages(2), 1);
    expect(layout.pages[0].top).toBe(PAGE_GAP);
    expect(layout.pages[1].top).toBe(PAGE_GAP + 792 + PAGE_GAP);
    expect(layout.totalHeight).toBe(2 * (792 + PAGE_GAP) + PAGE_GAP);
  });

  it("leaves room for the toolbar above the first page", () => {
    const layout = placePages(manyPages(2), 1, 60);
    expect(layout.pages[0].top).toBe(60 + PAGE_GAP);
    expect(layout.totalHeight).toBe(60 + 2 * (792 + PAGE_GAP) + PAGE_GAP);
  });

  it("scales pages with the zoom", () => {
    const layout = placePages([LETTER], 2);
    expect(layout.pages[0].width).toBe(1224);
    expect(layout.widestRow).toBe(1224);
  });

  it("centres each page on the content's centre line", () => {
    const layout = placePages([LETTER, { width: 300, height: 400 }], 1);
    expect(layout.pages[0].left).toBe(-306);
    expect(layout.pages[1].left).toBe(-150);
  });

  it("gives every page in single mode a row of its own", () => {
    const layout = placePages(manyPages(2), 1);
    expect(layout.pages[1]).toMatchObject({ rowTop: layout.pages[1].top, rowHeight: 792 });
  });
});

describe("rowsOf", () => {
  it("pairs pages in two page mode with a last odd page alone", () => {
    expect(rowsOf(5, "twoPages")).toEqual([[0, 1], [2, 3], [4]]);
    expect(rowsOf(4, "twoPages")).toEqual([
      [0, 1],
      [2, 3],
    ]);
    expect(rowsOf(3, "single")).toEqual([[0], [1], [2]]);
    expect(rowsOf(0, "twoPages")).toEqual([]);
  });
});

describe("two page mode", () => {
  const twoUp = (sizes: PageSize[], zoom = 1) => placePages(sizes, zoom, 0, "twoPages");

  it("puts a pair side by side, centred, with a gap between", () => {
    const { pages, widestRow } = twoUp(manyPages(2));
    expect(pages[0].top).toBe(pages[1].top);
    expect(pages[0].left).toBe(-(612 + PAGE_GAP / 2));
    expect(pages[1].left).toBe(PAGE_GAP / 2);
    expect(widestRow).toBe(2 * 612 + PAGE_GAP);
  });

  it("stacks rows with a gap between them", () => {
    const { pages, totalHeight } = twoUp(manyPages(4));
    expect(pages[2].top).toBe(PAGE_GAP + 792 + PAGE_GAP);
    expect(totalHeight).toBe(2 * (792 + PAGE_GAP) + PAGE_GAP);
  });

  it("keeps a last odd page in the left seat", () => {
    const { pages } = twoUp(manyPages(3));
    expect(pages[2].left).toBe(pages[0].left);
    expect(pages[2].rowTop).toBeGreaterThan(pages[0].rowTop);
  });

  it("centres mixed page sizes as a pair and the shorter one vertically", () => {
    const small = { width: 300, height: 400 };
    const { pages, widestRow } = twoUp([LETTER, small]);
    const rowWidth = 612 + PAGE_GAP + 300;
    expect(widestRow).toBe(rowWidth);
    expect(pages[0].left).toBe(-rowWidth / 2);
    expect(pages[1].left).toBe(-rowWidth / 2 + 612 + PAGE_GAP);
    expect(pages[0].rowHeight).toBe(792);
    expect(pages[1].top).toBe(PAGE_GAP + (792 - 400) / 2);
  });

  it("finds the row around a page", () => {
    const { pages } = twoUp(manyPages(5));
    expect(rowAround(pages, 3)).toEqual({ first: 2, last: 3 });
    expect(rowAround(pages, 4)).toEqual({ first: 4, last: 4 });
    expect(rowAround(placePages(manyPages(3), 1).pages, 1)).toEqual({ first: 1, last: 1 });
  });

  it("shows whole pairs as visible", () => {
    const { pages } = twoUp(manyPages(500));
    const top = pages[300].rowTop + 10;
    expect(visiblePages(pages, top, 600, 0)).toEqual({ first: 300, last: 301 });
    expect(visiblePages(pages, 0, 600, 0)).toEqual({ first: 0, last: 1 });
  });

  it("finds visible pages when the pair has different heights", () => {
    // the short right page ends before the screen top, its taller partner does not
    const sizes = [LETTER, { width: 300, height: 100 }, LETTER, LETTER];
    const { pages } = twoUp(sizes);
    const y = pages[0].top + 700;
    expect(visiblePages(pages, y, 50, 0)).toEqual({ first: 0, last: 1 });
  });

  it("names the left page of the middle row as current", () => {
    const { pages } = twoUp(manyPages(6));
    expect(pageAtMiddle(pages, pages[2].rowTop, 600)).toBe(2);
  });

  it("fits a pair to the window", () => {
    const zoom = zoomToFitWidth(manyPages(4), 2 * 612 + 3 * PAGE_GAP, "twoPages");
    expect(zoom).toBeCloseTo(1);
    const layout = placePages(manyPages(4), zoom, 0, "twoPages");
    expect(layout.widestRow + 2 * PAGE_GAP).toBeCloseTo(2 * 612 + 3 * PAGE_GAP);
  });

  it("fits the widest pair and leaves room for a lone last page", () => {
    // the lone wide last page needs room for two of itself
    const sizes = [LETTER, LETTER, { width: 1000, height: 792 }];
    const zoom = zoomToFitWidth(sizes, 2000 + 3 * PAGE_GAP, "twoPages");
    expect(zoom).toBeCloseTo(1);
  });
});

describe("anchors across page modes", () => {
  it("keep the same spot when switching to two pages and back at another zoom", () => {
    const single = placePages(manyPages(9), 1, 60).pages;
    const anchor = anchorAt(single, single[6].top + 792 / 4);
    expect(anchor).toEqual({ page: 6, fraction: 0.25 });

    const twoUp = placePages(manyPages(9), 0.7, 60, "twoPages").pages;
    const y = positionOf(twoUp, anchor);
    expect(y).toBeCloseTo(twoUp[6].top + (792 * 0.7) / 4);
    const again = anchorAt(twoUp, y);
    expect(again.page).toBe(6);
    expect(again.fraction).toBeCloseTo(0.25);

    const back = placePages(manyPages(9), 1.5, 60).pages;
    expect(positionOf(back, anchorAt(twoUp, y))).toBeCloseTo(back[6].top + (792 * 1.5) / 4);
  });

  it("round-trip on the last odd page", () => {
    const twoUp = placePages(manyPages(5), 2, 0, "twoPages").pages;
    const anchor = anchorAt(twoUp, twoUp[4].top + 100);
    expect(anchor.page).toBe(4);
    const single = placePages(manyPages(5), 1).pages;
    expect(positionOf(single, anchor)).toBeCloseTo(single[4].top + 50);
  });
});

describe("scrollTopForPage", () => {
  it("puts the page's row just below the toolbar", () => {
    const pages = placePages(manyPages(4), 1, 60, "twoPages").pages;
    expect(scrollTopForPage(pages, 3, 60)).toBe(pages[2].rowTop - PAGE_GAP - 60);
  });

  it("clamps page numbers that do not exist", () => {
    const pages = placePages(manyPages(3), 1).pages;
    expect(scrollTopForPage(pages, 99, 0)).toBe(pages[2].top - PAGE_GAP);
    expect(scrollTopForPage(pages, -4, 0)).toBe(0);
    expect(scrollTopForPage([], 2, 0)).toBe(0);
  });
});

describe("scrollToReveal", () => {
  it("leaves a span that is already in view alone", () => {
    expect(scrollToReveal(120, 50, 100, 300)).toBeNull();
  });

  it("scrolls up or down just enough", () => {
    expect(scrollToReveal(50, 20, 100, 300)).toBe(50);
    expect(scrollToReveal(450, 20, 100, 300)).toBe(170);
    expect(scrollToReveal(450, 20, 100, 300, 10)).toBe(180);
  });

  it("lines up a span bigger than the view with the view's start", () => {
    expect(scrollToReveal(500, 900, 0, 300)).toBe(500);
  });
});

describe("scrollToCentre", () => {
  it("brings a span out of view to the middle", () => {
    // a 20 pixel match at 1000 in a 300 pixel view: 140 above it, 140 below
    expect(scrollToCentre(1000, 20, 0, 300)).toBe(860);
  });

  it("leaves a span in view alone and never scrolls above the top", () => {
    expect(scrollToCentre(100, 20, 0, 300)).toBeNull();
    expect(scrollToCentre(10, 20, 500, 300)).toBe(0);
  });
});

describe("visiblePages", () => {
  const pages = placePages(manyPages(500), 1).pages;

  it("finds only the pages on screen", () => {
    expect(visiblePages(pages, 0, 600, 0)).toEqual({ first: 0, last: 0 });
    // scrolled so the gap between page 0 and page 1 is on screen
    expect(visiblePages(pages, 700, 600, 0)).toEqual({ first: 0, last: 1 });
  });

  it("works deep inside a long document", () => {
    const top = pages[300].top + 10;
    expect(visiblePages(pages, top, 600, 0)).toEqual({ first: 300, last: 300 });
  });

  it("includes neighbours inside the margin", () => {
    const top = pages[300].top;
    expect(visiblePages(pages, top, 600, 400)).toEqual({ first: 299, last: 301 });
  });

  it("returns nothing for an empty document or past the end", () => {
    expect(visiblePages([], 0, 600, 0)).toBeNull();
    expect(visiblePages(pages, 10_000_000, 600, 0)).toBeNull();
  });
});

describe("pageAtMiddle", () => {
  const pages = placePages(manyPages(3), 1).pages;

  it("names the page under the middle of the screen", () => {
    expect(pageAtMiddle(pages, 0, 600)).toBe(0);
    expect(pageAtMiddle(pages, 792, 600)).toBe(1);
  });

  it("stays on the last page at the very bottom", () => {
    expect(pageAtMiddle(pages, 100_000, 600)).toBe(2);
  });
});

describe("anchors", () => {
  it("keep the same spot of the same page when the zoom changes", () => {
    const before = placePages(manyPages(500), 1, 60).pages;
    const after = placePages(manyPages(500), 2.3, 60).pages;
    // a third of the way down page 250 (index 249)
    const anchor = anchorAt(before, before[249].top + 792 / 3);
    expect(anchor.page).toBe(249);
    expect(anchor.fraction).toBeCloseTo(1 / 3);
    expect(positionOf(after, anchor)).toBeCloseTo(after[249].top + (792 * 2.3) / 3);
  });

  it("handle the gap above a page and an empty document", () => {
    const pages = placePages(manyPages(2), 1).pages;
    // a point in the gap belongs to the page below it, slightly above its top
    expect(anchorAt(pages, pages[1].top - 5)).toEqual({ page: 1, fraction: -5 / 792 });
    expect(positionOf([], anchorAt([], 100))).toBe(0);
  });
});

describe("pagePixelSize", () => {
  it("multiplies points by the scale", () => {
    expect(pagePixelSize(LETTER, 1000)).toEqual({ width: 612, height: 792 });
    expect(pagePixelSize(LETTER, 2000)).toEqual({ width: 1224, height: 1584 });
  });

  it("rounds up like the engine does", () => {
    expect(pagePixelSize(LETTER, 1501).width).toBe(919);
    expect(pagePixelSize(LETTER, 1502).width).toBe(920);
  });
});

describe("scaleMilli", () => {
  it("accounts for high density screens", () => {
    expect(scaleMilli(1, 1)).toBe(1000);
    expect(scaleMilli(1.5, 2)).toBe(3000);
  });
});

describe("tilesForPage", () => {
  it("covers a whole small page with trimmed edge tiles", () => {
    const tiles = tilesForPage(LETTER, 1000, 1, 0, 792);
    expect(tiles).toHaveLength(4);
    const corner = tiles.find((tile) => tile.column === 1 && tile.row === 1)!;
    expect(corner).toMatchObject({ left: 512, top: 512, width: 100, height: 280 });
  });

  it("only returns the rows that are on screen", () => {
    // at 8x the page is 6336 pixels tall: 13 rows of tiles
    const tiles = tilesForPage(LETTER, 8000, 1, 1100, 1700);
    const rows = [...new Set(tiles.map((tile) => tile.row))];
    expect(rows).toEqual([2, 3]);
  });

  it("returns nothing when the page is off screen", () => {
    expect(tilesForPage(LETTER, 1000, 1, -500, -10)).toEqual([]);
    expect(tilesForPage(LETTER, 1000, 1, 900, 1200)).toEqual([]);
  });

  it("draws tiles at css size on high density screens", () => {
    const tiles = tilesForPage(LETTER, 2000, 2, 0, 792);
    expect(tiles[0]).toMatchObject({ width: 256, height: 256 });
  });
});

describe("zoom", () => {
  it("fits the widest page to the window", () => {
    const zoom = zoomToFitWidth([LETTER, { width: 1224, height: 792 }], 1224 + 2 * PAGE_GAP);
    expect(zoom).toBe(1);
  });

  it("never leaves the allowed range", () => {
    let zoom = 1;
    for (let step = 0; step < 100; step += 1) zoom = zoomIn(zoom);
    expect(zoom).toBe(MAX_ZOOM);
    for (let step = 0; step < 200; step += 1) zoom = zoomOut(zoom);
    expect(zoom).toBe(MIN_ZOOM);
  });
});

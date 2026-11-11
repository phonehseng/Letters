import { describe, expect, it } from "vitest";
import {
  MAX_ZOOM,
  MIN_ZOOM,
  PAGE_GAP,
  pageAtMiddle,
  pagePixelSize,
  placePages,
  scaleMilli,
  tilesForPage,
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

  it("scales pages with the zoom", () => {
    const layout = placePages([LETTER], 2);
    expect(layout.pages[0].width).toBe(1224);
    expect(layout.widestPage).toBe(1224);
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

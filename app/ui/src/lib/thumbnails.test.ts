import { describe, expect, it } from "vitest";
import { PAGE_GAP, visiblePages } from "./layout";
import { THUMBNAIL_BOX, thumbnailLayout, thumbnailTiles, thumbnailZoom } from "./thumbnails";

const LETTER = { width: 612, height: 792 };
const LABEL = 20;

describe("thumbnailZoom", () => {
  it("makes a usual page 120 css pixels wide", () => {
    expect(LETTER.width * thumbnailZoom(LETTER)).toBeCloseTo(THUMBNAIL_BOX.width);
  });

  it("fits a very tall page to the box's height", () => {
    const receipt = { width: 200, height: 2000 };
    expect(receipt.height * thumbnailZoom(receipt)).toBeCloseTo(THUMBNAIL_BOX.height);
  });
});

describe("thumbnailLayout", () => {
  it("stacks thumbnails with room for the page number", () => {
    const layout = thumbnailLayout([LETTER, LETTER], LABEL);
    const pictureHeight = (792 * 120) / 612;
    expect(layout.pages[0].height).toBeCloseTo(pictureHeight + LABEL);
    expect(layout.pages[1].top).toBeCloseTo(PAGE_GAP + pictureHeight + LABEL + PAGE_GAP);
  });

  it("puts only a handful of a 500 page document on screen", () => {
    const sizes = Array.from({ length: 500 }, () => LETTER);
    const layout = thumbnailLayout(sizes, LABEL);
    const range = visiblePages(layout.pages, layout.pages[250].top, 700, 350)!;
    const shown = range.last - range.first + 1;
    expect(shown).toBeLessThan(12);
    expect(range.first).toBeLessThanOrEqual(250);
    expect(range.last).toBeGreaterThanOrEqual(250);
  });
});

describe("thumbnailTiles", () => {
  it("uses one tile for a usual page, even on a high density screen", () => {
    const thumbnail = thumbnailTiles(LETTER, 2);
    expect(thumbnail.tiles).toHaveLength(1);
    expect(thumbnail.width).toBeCloseTo(120, 0);
    expect(thumbnail.scale).toBe(Math.round((120 / 612) * 2 * 1000));
  });

  it("covers a page too big for one tile with the tiles it needs", () => {
    // at a density of 5 the thumbnail is 600 device pixels wide: two columns
    const thumbnail = thumbnailTiles(LETTER, 5);
    const columns = new Set(thumbnail.tiles.map((tile) => tile.column));
    const rows = new Set(thumbnail.tiles.map((tile) => tile.row));
    expect(columns.size).toBe(2);
    expect(rows.size).toBe(2);
  });
});


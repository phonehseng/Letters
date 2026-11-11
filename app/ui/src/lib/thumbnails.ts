// the arithmetic of the thumbnails list in the sidebar.
//
// thumbnails are small pictures of pages, made from the same tiles as the
// pages themselves, just at a small scale. the list reuses the page layout
// functions, so it gets binary search and "only what is on screen" for free.

import { pagePixelSize, placePages, scaleMilli, tilesForPage, type PageLayout, type PageSize, type TileSpot } from "./layout";

/** a thumbnail fits inside this box, in css pixels: about 120 wide for usual pages. */
export const THUMBNAIL_BOX = { width: 120, height: 180 };

/**
 * how big a thumbnail is drawn, in css pixels per point. a very tall
 * page (a receipt, a poster) shrinks to fit the box's height instead
 * of making one thumbnail as long as the whole list.
 */
export function thumbnailZoom(size: PageSize, box = THUMBNAIL_BOX): number {
  if (size.width <= 0 || size.height <= 0) return 1;
  return Math.min(box.width / size.width, box.height / size.height);
}

/**
 * where each thumbnail sits in the list. `labelHeight` is the room under
 * each picture for its page number, in css pixels.
 */
export function thumbnailLayout(sizes: PageSize[], labelHeight: number, box = THUMBNAIL_BOX): PageLayout {
  const slots = sizes.map((size) => {
    const zoom = thumbnailZoom(size, box);
    return { width: size.width * zoom, height: size.height * zoom + labelHeight };
  });
  return placePages(slots, 1);
}

/** the render scale and the tiles that together make one thumbnail. */
export function thumbnailTiles(
  size: PageSize,
  devicePixelRatio: number,
  box = THUMBNAIL_BOX,
): { scale: number; width: number; height: number; tiles: TileSpot[] } {
  const scale = scaleMilli(thumbnailZoom(size, box), devicePixelRatio);
  const pixels = pagePixelSize(size, scale);
  // a usual page fits in one 512 pixel tile; a large one gets the few it needs
  const tiles = tilesForPage(size, scale, devicePixelRatio, 0, Number.MAX_SAFE_INTEGER);
  return {
    scale,
    width: pixels.width / devicePixelRatio,
    height: pixels.height / devicePixelRatio,
    tiles,
  };
}

/**
 * the page to go to after a key in the thumbnails list, or null.
 *
 * `row` is the current row of the document (two pages in two page mode),
 * so the down arrow moves past both pages of a pair instead of staying put.
 */
export function thumbnailKeyTarget(key: string, row: { first: number; last: number }, pageCount: number): number | null {
  if (pageCount === 0) return null;
  if (key === "ArrowDown") return Math.min(row.last + 1, pageCount - 1);
  if (key === "ArrowUp") return Math.max(row.first - 1, 0);
  if (key === "Home") return 0;
  if (key === "End") return pageCount - 1;
  return null;
}

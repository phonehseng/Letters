// the arithmetic behind the viewer: where each page sits in the scroll
// area, which pages are on screen, and which tiles those pages need.
//
// nothing here touches the screen, so all of it is unit tested.

/** a page's size in points (1/72 inch), as stored in the pdf. */
export interface PageSize {
  width: number;
  height: number;
}

/** where a page sits in the scroll area, in css pixels. */
export interface PlacedPage {
  index: number;
  top: number;
  width: number;
  height: number;
}

export interface PageLayout {
  pages: PlacedPage[];
  totalHeight: number;
  widestPage: number;
}

/** one tile of a page: its grid position and where to draw it, in css pixels. */
export interface TileSpot {
  column: number;
  row: number;
  left: number;
  top: number;
  width: number;
  height: number;
}

/** must match TILE_SIZE in core/pdf/src/geometry.rs. */
export const TILE_SIZE = 512;

/** the space between pages, and above the first one. */
export const PAGE_GAP = 16;

export const MIN_ZOOM = 0.1;
export const MAX_ZOOM = 8;

/** each press of zoom in or out changes the size by this factor. */
const ZOOM_STEP = 1.1;

/**
 * stacks the pages top to bottom. zoom is css pixels per point.
 *
 * `topInset` leaves room above the first page for the floating toolbar,
 * so the first page starts below it but later pages scroll underneath.
 */
export function placePages(sizes: PageSize[], zoom: number, topInset = 0): PageLayout {
  const pages: PlacedPage[] = [];
  let top = topInset + PAGE_GAP;
  let widestPage = 0;
  sizes.forEach((size, index) => {
    const width = size.width * zoom;
    const height = size.height * zoom;
    pages.push({ index, top, width, height });
    top += height + PAGE_GAP;
    widestPage = Math.max(widestPage, width);
  });
  return { pages, totalHeight: top, widestPage };
}

/**
 * the first and last page that overlap the screen, or null for an empty document.
 *
 * `margin` widens the screen a little in both directions so pages are
 * ready just before they scroll into view.
 */
export function visiblePages(
  pages: PlacedPage[],
  scrollTop: number,
  viewportHeight: number,
  margin: number,
): { first: number; last: number } | null {
  if (pages.length === 0) return null;
  const top = scrollTop - margin;
  const bottom = scrollTop + viewportHeight + margin;

  const first = firstPageEndingAfter(pages, top);
  if (first >= pages.length) return null;
  let last = first;
  while (last + 1 < pages.length && pages[last + 1].top < bottom) last += 1;
  if (pages[first].top >= bottom) return null;
  return { first, last };
}

/**
 * binary search, so a 500 page document costs about 9 steps per scroll
 * instead of 500.
 */
function firstPageEndingAfter(pages: PlacedPage[], y: number): number {
  let low = 0;
  let high = pages.length;
  while (low < high) {
    const middle = (low + high) >> 1;
    if (pages[middle].top + pages[middle].height <= y) low = middle + 1;
    else high = middle;
  }
  return low;
}

/** the page that covers the middle of the screen: the "current page". */
export function pageAtMiddle(pages: PlacedPage[], scrollTop: number, viewportHeight: number): number {
  if (pages.length === 0) return 0;
  const index = firstPageEndingAfter(pages, scrollTop + viewportHeight / 2);
  return Math.min(index, pages.length - 1);
}

/** a place in the document that survives a change of zoom. */
export interface Anchor {
  page: number;
  /** how far down that page, from 0 (its top edge) to 1 (its bottom edge) */
  fraction: number;
}

/**
 * describes the height `y` of the scroll area as "this far down that page".
 *
 * pixels change meaning when the zoom changes, but "a third of the way
 * down page 250" does not, so this is what we remember across a zoom.
 */
export function anchorAt(pages: PlacedPage[], y: number): Anchor {
  if (pages.length === 0) return { page: 0, fraction: 0 };
  const index = Math.min(firstPageEndingAfter(pages, y), pages.length - 1);
  const page = pages[index];
  return { page: index, fraction: (y - page.top) / page.height };
}

/** the height in the scroll area where an anchor now sits. */
export function positionOf(pages: PlacedPage[], anchor: Anchor): number {
  const page = pages[anchor.page];
  if (!page) return 0;
  return page.top + anchor.fraction * page.height;
}

/**
 * the render scale in thousandths of a device pixel per point.
 *
 * on a high density screen one css pixel is several device pixels, so
 * the scale is zoom times that density. whole thousandths are used
 * because the engine identifies tiles by an exact integer scale.
 */
export function scaleMilli(zoom: number, devicePixelRatio: number): number {
  return Math.max(1, Math.round(zoom * devicePixelRatio * 1000));
}

/**
 * the page's size in device pixels. must give the same answer as
 * page_pixel_size in core/pdf/src/geometry.rs, which uses 32 bit
 * floats, so the same rounding is applied here with Math.fround.
 */
export function pagePixelSize(size: PageSize, scale: number): { width: number; height: number } {
  const factor = Math.fround(scale / 1000);
  const toPixels = (points: number) =>
    Math.max(1, Math.ceil(Math.fround(Math.fround(points) * factor)));
  return { width: toPixels(size.width), height: toPixels(size.height) };
}

/**
 * the tiles of one page that overlap the visible band.
 *
 * `visibleTop` and `visibleBottom` are css pixels measured from the top
 * of the page. tiles outside the band are skipped, so a page zoomed far
 * in only loads the part that is actually on screen.
 */
export function tilesForPage(
  size: PageSize,
  scale: number,
  devicePixelRatio: number,
  visibleTop: number,
  visibleBottom: number,
): TileSpot[] {
  const pixels = pagePixelSize(size, scale);
  const toCss = (devicePixels: number) => devicePixels / devicePixelRatio;
  const columns = Math.ceil(pixels.width / TILE_SIZE);
  const rows = Math.ceil(pixels.height / TILE_SIZE);
  const firstRow = clamp(Math.floor((visibleTop * devicePixelRatio) / TILE_SIZE), 0, rows - 1);
  const lastRow = clamp(Math.floor((visibleBottom * devicePixelRatio) / TILE_SIZE), 0, rows - 1);

  const tiles: TileSpot[] = [];
  if (visibleBottom <= 0 || visibleTop >= toCss(pixels.height)) return tiles;
  for (let row = firstRow; row <= lastRow; row += 1) {
    for (let column = 0; column < columns; column += 1) {
      const x = column * TILE_SIZE;
      const y = row * TILE_SIZE;
      tiles.push({
        column,
        row,
        left: toCss(x),
        top: toCss(y),
        width: toCss(Math.min(TILE_SIZE, pixels.width - x)),
        height: toCss(Math.min(TILE_SIZE, pixels.height - y)),
      });
    }
  }
  return tiles;
}

/** the zoom at which the widest page exactly fills the available width. */
export function zoomToFitWidth(sizes: PageSize[], availableWidth: number): number {
  const widest = Math.max(0, ...sizes.map((size) => size.width));
  if (widest === 0 || availableWidth <= 0) return 1;
  return clampZoom((availableWidth - 2 * PAGE_GAP) / widest);
}

export function zoomIn(zoom: number): number {
  return clampZoom(zoom * ZOOM_STEP);
}

export function zoomOut(zoom: number): number {
  return clampZoom(zoom / ZOOM_STEP);
}

export function clampZoom(zoom: number): number {
  return clamp(zoom, MIN_ZOOM, MAX_ZOOM);
}

function clamp(value: number, low: number, high: number): number {
  return Math.min(high, Math.max(low, value));
}

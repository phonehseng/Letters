// the arithmetic behind the viewer: where each page sits in the scroll
// area, which pages are on screen, and which tiles those pages need.
//
// nothing here touches the screen, so all of it is unit tested.

/** a page's size in points (1/72 inch), as stored in the pdf. */
export interface PageSize {
  width: number;
  height: number;
}

/** one page under another, or pages side by side in pairs like an open book. */
export type PageMode = "single" | "twoPages";

/** where a page sits in the scroll area, in css pixels. */
export interface PlacedPage {
  index: number;
  top: number;
  /** the page's left edge, measured from the centre line of the content */
  left: number;
  width: number;
  height: number;
  /**
   * the band of the row this page belongs to. in single mode a row is
   * just the page. pages of one row share the band, which keeps the
   * band's bottom edge growing from page to page so binary search works
   * even when the two pages of a row have different heights.
   */
  rowTop: number;
  rowHeight: number;
}

export interface PageLayout {
  pages: PlacedPage[];
  totalHeight: number;
  /** the widest row of pages, including the gap inside a pair */
  widestRow: number;
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

/** the space between pages, above the first one, and between the pages of a pair. */
export const PAGE_GAP = 16;

export const MIN_ZOOM = 0.1;
export const MAX_ZOOM = 8;

/** each press of zoom in or out changes the size by this factor. */
const ZOOM_STEP = 1.1;

/**
 * which pages share a row, as lists of page indexes.
 *
 * in two page mode page 1 sits with page 2, page 3 with page 4, and a
 * last odd page gets a row of its own.
 */
export function rowsOf(pageCount: number, mode: PageMode): number[][] {
  const perRow = mode === "twoPages" ? 2 : 1;
  const rows: number[][] = [];
  for (let first = 0; first < pageCount; first += perRow) {
    rows.push(Array.from({ length: Math.min(perRow, pageCount - first) }, (_, offset) => first + offset));
  }
  return rows;
}

/**
 * how wide a row is at a zoom, in css pixels.
 *
 * a page alone in a two page row is measured as if it had a partner of
 * the same size, because it keeps the left seat instead of moving to
 * the middle (so it lines up with the left pages above it).
 */
function rowWidth(widths: number[], mode: PageMode): number {
  if (mode === "single") return widths[0];
  const [left, right] = widths;
  return right === undefined ? 2 * left + PAGE_GAP : left + PAGE_GAP + right;
}

/**
 * lays the pages out top to bottom, one per row or two per row. zoom is
 * css pixels per point.
 *
 * `topInset` leaves room above the first page for the floating toolbar,
 * so the first page starts below it but later pages scroll underneath.
 */
export function placePages(sizes: PageSize[], zoom: number, topInset = 0, mode: PageMode = "single"): PageLayout {
  const pages: PlacedPage[] = [];
  let top = topInset + PAGE_GAP;
  let widestRow = 0;
  for (const row of rowsOf(sizes.length, mode)) {
    const widths = row.map((index) => sizes[index].width * zoom);
    const heights = row.map((index) => sizes[index].height * zoom);
    const width = rowWidth(widths, mode);
    const height = Math.max(...heights);
    // the row is centred on the content's centre line
    let left = -width / 2;
    row.forEach((index, seat) => {
      // a shorter page sits in the middle of the row's height, like an open book
      const pageTop = top + (height - heights[seat]) / 2;
      pages.push({ index, top: pageTop, left, width: widths[seat], height: heights[seat], rowTop: top, rowHeight: height });
      left += widths[seat] + PAGE_GAP;
    });
    top += height + PAGE_GAP;
    widestRow = Math.max(widestRow, width);
  }
  return { pages, totalHeight: top, widestRow };
}

/** the first and last page index of the row that holds this page. */
export function rowAround(pages: PlacedPage[], index: number): { first: number; last: number } {
  if (pages.length === 0) return { first: 0, last: 0 };
  const clamped = clamp(index, 0, pages.length - 1);
  const rowTop = pages[clamped].rowTop;
  let first = clamped;
  let last = clamped;
  while (first > 0 && pages[first - 1].rowTop === rowTop) first -= 1;
  while (last + 1 < pages.length && pages[last + 1].rowTop === rowTop) last += 1;
  return { first, last };
}

/**
 * the first and last page that overlap the screen, or null for an empty document.
 *
 * `margin` widens the screen a little in both directions so pages are
 * ready just before they scroll into view. a page counts as on screen
 * when its row is, so both pages of a pair come and go together.
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
  while (last + 1 < pages.length && pages[last + 1].rowTop < bottom) last += 1;
  if (pages[first].rowTop >= bottom) return null;
  return { first, last };
}

/**
 * binary search, so a 500 page document costs about 9 steps per scroll
 * instead of 500. it looks at row bands, which only ever grow downwards.
 */
function firstPageEndingAfter(pages: PlacedPage[], y: number): number {
  let low = 0;
  let high = pages.length;
  while (low < high) {
    const middle = (low + high) >> 1;
    if (pages[middle].rowTop + pages[middle].rowHeight <= y) low = middle + 1;
    else high = middle;
  }
  return low;
}

/**
 * the page that covers the middle of the screen: the "current page".
 * in two page mode it is the left page of the row in the middle.
 */
export function pageAtMiddle(pages: PlacedPage[], scrollTop: number, viewportHeight: number): number {
  if (pages.length === 0) return 0;
  const index = firstPageEndingAfter(pages, scrollTop + viewportHeight / 2);
  return Math.min(index, pages.length - 1);
}

/** a place in the document that survives a change of zoom or page mode. */
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
 * where to scroll so a page's row starts just below the floating toolbar.
 * the page number is clamped, so a typo like 9999 goes to the last page.
 */
export function scrollTopForPage(pages: PlacedPage[], index: number, toolbarHeight: number): number {
  if (pages.length === 0) return 0;
  const page = pages[clamp(Math.round(index), 0, pages.length - 1)];
  return Math.max(0, page.rowTop - PAGE_GAP - toolbarHeight);
}

/**
 * the smallest scroll that brings a span fully into view, or null when it
 * is already in view (so nothing moves needlessly).
 *
 * works for either direction: pass tops and heights, or lefts and widths.
 * a span bigger than the view is lined up with the view's start.
 */
export function scrollToReveal(
  spanStart: number,
  spanSize: number,
  viewStart: number,
  viewSize: number,
  margin = 0,
): number | null {
  const start = spanStart - margin;
  const end = spanStart + spanSize + margin;
  if (start >= viewStart && end <= viewStart + viewSize) return null;
  if (end - start > viewSize || start < viewStart) return Math.max(0, start);
  return Math.max(0, end - viewSize);
}

/**
 * like scrollToReveal, but a span that is not fully in view is brought to
 * the middle of the view. used for find, where the eye should not have
 * to hunt along the edge of the window for the match.
 */
export function scrollToCentre(spanStart: number, spanSize: number, viewStart: number, viewSize: number): number | null {
  if (scrollToReveal(spanStart, spanSize, viewStart, viewSize) === null) return null;
  return Math.max(0, spanStart - (viewSize - spanSize) / 2);
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

/**
 * the zoom at which the widest row exactly fills the available width:
 * the widest page in single mode, the widest pair in two page mode.
 */
export function zoomToFitWidth(sizes: PageSize[], availableWidth: number, mode: PageMode = "single"): number {
  if (sizes.length === 0 || availableWidth <= 0) return 1;
  // a row is (its page widths in points) * zoom plus gaps that do not
  // scale: one each side, and one inside a pair. so the zoom that fits
  // the widest row is the room left after the gaps over its points.
  const gaps = mode === "twoPages" ? 3 * PAGE_GAP : 2 * PAGE_GAP;
  const widestPoints = Math.max(
    ...rowsOf(sizes.length, mode).map((row) => {
      const [left, right] = row.map((index) => sizes[index].width);
      // a lone last page keeps the left seat, so it needs room for two
      return mode === "twoPages" ? left + (right ?? left) : left;
    }),
  );
  if (widestPoints <= 0) return 1;
  return clampZoom((availableWidth - gaps) / widestPoints);
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

// side by side: where each page sits in the two columns, and keeping the
// columns in step as either one scrolls.
//
// both columns have one slot per page pair, in the same order, so pair 7
// is the seventh slot on both sides. a page one side doesn't have gets a
// placeholder the size of the other side's page ("Not in the old
// version"), which keeps the slots lined up. within a pair, the engine's
// scroll map says which height on the new page matches a height on the
// old one, so a paragraph pushed down by an inserted sentence stays level
// with its old self.

import { anchorAt, placePages, type PageLayout, type PageSize } from "../layout";
import type { PagePair, ScrollPoint } from "./compareApi";
import type { Side } from "./changes";

const LETTER: PageSize = { width: 612, height: 792 };

/** one side's slot of a pair: its own page, or a placeholder. */
export interface Slot {
  pair: number;
  /** the side's page, or null for a placeholder */
  page: number | null;
  size: PageSize;
}

/** the slots of one side, in pair order. */
export function slotsOf(pairs: PagePair[], side: Side): Slot[] {
  return pairs.map((pair, index) => {
    const page = side === "old" ? pair.leftPage : pair.rightPage;
    const own = side === "old" ? pair.leftSize : pair.rightSize;
    const other = side === "old" ? pair.rightSize : pair.leftSize;
    return { pair: index, page, size: own ?? other ?? LETTER };
  });
}

/**
 * the overlay's slot for each pair: big enough for both pages, because the
 * engine's lined-up pictures are as big as the larger of the two.
 */
export function overlaySizesOf(pairs: PagePair[]): PageSize[] {
  return pairs.map((pair) => {
    const sizes = [pair.leftSize, pair.rightSize].filter((size): size is PageSize => size !== null);
    if (sizes.length === 0) return LETTER;
    const width = Math.max(...sizes.map((size) => size.width));
    const height = Math.max(...sizes.map((size) => size.height));
    return { width, height };
  });
}

/** where the slots sit in a column, using the viewer's layout rules. */
export function columnLayout(slots: Slot[], zoom: number, topInset: number): PageLayout {
  return placePages(
    slots.map((slot) => slot.size),
    zoom,
    topInset,
    "single",
  );
}

/** which slot shows a side's page, by page number. */
export function slotOfPage(slots: Slot[], page: number | null): number {
  if (page === null) return -1;
  return slots.findIndex((slot) => slot.page === page);
}

/**
 * the height on the new page that matches `leftY` on the old page. the
 * same arithmetic as `map_scroll` in core/compare's result.rs: straight
 * lines between the map's points, and the same distance past either end.
 */
export function mapScroll(map: ScrollPoint[], leftY: number): number {
  if (map.length === 0) return leftY;
  const first = map[0];
  if (leftY <= first.leftY) return first.rightY + (leftY - first.leftY);
  for (let index = 1; index < map.length; index += 1) {
    const a = map[index - 1];
    const b = map[index];
    if (leftY <= b.leftY) {
      const span = b.leftY - a.leftY;
      if (span <= Number.EPSILON) return b.rightY;
      return a.rightY + ((leftY - a.leftY) / span) * (b.rightY - a.rightY);
    }
  }
  const last = map[map.length - 1];
  return last.rightY + (leftY - last.leftY);
}

/** the map the other way round, for scrolling the new side: sorted by the new page's heights. */
export function invertMap(map: ScrollPoint[]): ScrollPoint[] {
  return map.map((point) => ({ leftY: point.rightY, rightY: point.leftY })).sort((a, b) => a.leftY - b.leftY);
}

/**
 * the height (in points) on the other side's slot that matches `y` points
 * down this side's slot of the same pair. without a page on both sides
 * there is no map, so the same share of the page is used.
 */
export function matchingHeight(pair: PagePair, from: Side, y: number, fromSize: PageSize, toSize: PageSize): number {
  const bothPages = pair.leftPage !== null && pair.rightPage !== null;
  if (!bothPages || pair.scrollMap.length === 0) return (y / fromSize.height) * toSize.height;
  const map = from === "old" ? pair.scrollMap : invertMap(pair.scrollMap);
  return mapScroll(map, y);
}

/**
 * where the other column should scroll so it shows what this one shows
 * just below the toolbar. `inset` is how far below the column's top that
 * line is.
 */
export function followingScrollTop(
  pairs: PagePair[],
  from: Side,
  fromLayout: PageLayout,
  toLayout: PageLayout,
  fromSlots: Slot[],
  toSlots: Slot[],
  scrollTop: number,
  inset: number,
  zoom: number,
): number {
  if (fromLayout.pages.length === 0 || toLayout.pages.length === 0) return 0;
  const anchor = anchorAt(fromLayout.pages, scrollTop + inset);
  const index = Math.min(anchor.page, toLayout.pages.length - 1);
  const fromSlot = fromSlots[index];
  const toSlot = toSlots[index];
  const y = anchor.fraction * fromSlot.size.height;
  const matched = matchingHeight(pairs[index], from, y, fromSlot.size, toSlot.size);
  return Math.max(0, toLayout.pages[index].top + matched * zoom - inset);
}

/**
 * the scroll that puts a rectangle (in points, on slot `slot`) a third of
 * the way down the visible part of a column: easy to find, with context
 * above it.
 */
export function scrollTopToShow(layout: PageLayout, slot: number, rectTop: number, zoom: number, inset: number, viewportHeight: number): number {
  const placed = layout.pages[slot];
  if (!placed) return 0;
  const visible = Math.max(1, viewportHeight - inset);
  return Math.max(0, placed.top + rectTop * zoom - inset - visible / 3);
}

// the maths of a list that only puts the rows on screen into the page,
// so a list of two thousand fonts scrolls as smoothly as one of twenty.
// every row has the same height; nothing here touches the dom.

export interface VisibleRows {
  /** the first row to draw */
  start: number;
  /** one past the last row to draw */
  end: number;
  /** where the first drawn row sits, in pixels from the top of the list */
  offset: number;
  /** the height of the whole list, so the scroll bar is right */
  total: number;
}

/**
 * the rows that overlap the viewport, plus `overscan` rows on each side
 * so a fast scroll doesn't show blank space before new rows arrive.
 */
export function visibleRows(scrollTop: number, viewport: number, rowHeight: number, count: number, overscan = 6): VisibleRows {
  const total = Math.max(0, count) * rowHeight;
  if (count <= 0 || rowHeight <= 0) return { start: 0, end: 0, offset: 0, total: 0 };
  const top = Math.max(0, Math.min(scrollTop, Math.max(0, total - viewport)));
  const first = Math.floor(top / rowHeight);
  const last = Math.ceil((top + Math.max(0, viewport)) / rowHeight);
  const start = Math.max(0, first - overscan);
  const end = Math.min(count, last + overscan);
  return { start, end, offset: start * rowHeight, total };
}

/**
 * the scroll position that brings row `index` fully into view, moving as
 * little as possible (none when it is already visible).
 */
export function scrollToShow(index: number, scrollTop: number, viewport: number, rowHeight: number): number {
  const top = index * rowHeight;
  const bottom = top + rowHeight;
  if (top < scrollTop) return top;
  if (bottom > scrollTop + viewport) return Math.max(0, bottom - viewport);
  return scrollTop;
}

/**
 * the height of a Font Library row in pixels: the name line, the sample
 * at its size (points are 4/3 of a css pixel) with room for accents and
 * descenders, and the padding.
 */
export function libraryRowHeight(sizePoints: number): number {
  return Math.ceil(sizePoints * (4 / 3) * 1.4) + 34;
}

/** how many whole rows fit in the viewport, at least one. */
export function rowsPerPage(viewport: number, rowHeight: number): number {
  return Math.max(1, Math.floor(viewport / Math.max(1, rowHeight)));
}

/**
 * the row a key moves to, or null for keys the list doesn't use. rows
 * that can't be chosen (group headings) are stepped over.
 */
export function moveWithKey(
  key: string,
  current: number,
  count: number,
  pageSize: number,
  selectable: (index: number) => boolean = () => true,
): number | null {
  if (count <= 0) return null;
  const step = (from: number, direction: 1 | -1) => {
    let at = from;
    while (at >= 0 && at < count && !selectable(at)) at += direction;
    if (at < 0 || at >= count) {
      // nothing selectable that way: look back the other way
      at = from - direction;
      while (at >= 0 && at < count && !selectable(at)) at -= direction;
    }
    return Math.max(0, Math.min(count - 1, at));
  };
  switch (key) {
    case "ArrowDown":
      return current < 0 ? step(0, 1) : step(Math.min(count - 1, current + 1), 1);
    case "ArrowUp":
      return current < 0 ? step(0, 1) : step(Math.max(0, current - 1), -1);
    case "PageDown":
      return step(Math.min(count - 1, Math.max(0, current) + pageSize), 1);
    case "PageUp":
      return step(Math.max(0, current - pageSize), -1);
    case "Home":
      return step(0, 1);
    case "End":
      return step(count - 1, -1);
    default:
      return null;
  }
}

// moving pages: where a dragged block lands, where the moved pages end
// up afterwards, and the drag's arithmetic in the thumbnails list.
//
// the list only builds the thumbnails near its visible part, so a drag
// can't rely on page elements being there: the thumbnail under the
// pointer may not exist yet, or may vanish mid-drag. everything here
// works from the list's layout numbers instead, which cover every page.

/** a thumbnail's slot in the list, in css pixels from the list's top. */
export interface Slot {
  top: number;
  height: number;
}

/**
 * the new order of the pages (old indexes in their new places) after
 * moving `moving` as a block to just before the page now at `before`.
 *
 * this matches move_pages in core/pages: dropping onto one of the moved
 * pages means "before the next page that stays".
 */
export function orderAfterMove(pageCount: number, moving: number[], before: number): number[] {
  const movingSet = new Set(moving);
  const block = [...movingSet].sort((a, b) => a - b);
  const stays = Array.from({ length: pageCount }, (_, page) => page).filter((page) => !movingSet.has(page));
  let anchor = before;
  while (anchor < pageCount && movingSet.has(anchor)) anchor += 1;
  const at = anchor >= pageCount ? stays.length : stays.indexOf(anchor);
  return [...stays.slice(0, at), ...block, ...stays.slice(at)];
}

/** where the moved pages are after the move, so they can stay selected. */
export function positionsAfterMove(pageCount: number, moving: number[], before: number): number[] {
  const order = orderAfterMove(pageCount, moving, before);
  const movingSet = new Set(moving);
  return order.flatMap((page, position) => (movingSet.has(page) ? [position] : []));
}

/** true when the drop would leave every page where it is. */
export function changesNothing(pageCount: number, moving: number[], before: number): boolean {
  return orderAfterMove(pageCount, moving, before).every((page, position) => page === position);
}

/**
 * the keyboard move (Alt + Up or Down): one place up or down, as a block.
 * returns the `before` index for the move, or null at the edge.
 */
export function stepTarget(moving: number[], direction: "up" | "down", pageCount: number): number | null {
  if (moving.length === 0) return null;
  const first = Math.min(...moving);
  const last = Math.max(...moving);
  if (direction === "up") return first === 0 ? null : first - 1;
  // before the page after the next one that stays; past the end is the end
  const order = Array.from({ length: pageCount }, (_, page) => page);
  const after = order.filter((page) => page > last && !moving.includes(page));
  if (after.length === 0) return null;
  return after[1] ?? pageCount;
}

/**
 * the insertion point for a pointer at `y` (css pixels from the top of
 * the list's content): before the first slot whose middle is below it.
 * a binary search, so a 500 page list costs about 9 steps.
 */
export function dropIndexAt(slots: Slot[], y: number): number {
  let low = 0;
  let high = slots.length;
  while (low < high) {
    const middle = (low + high) >> 1;
    if (slots[middle].top + slots[middle].height / 2 < y) low = middle + 1;
    else high = middle;
  }
  return low;
}

/**
 * where the insertion line is drawn for a drop index: in the middle of
 * the gap above that slot, or below the last one.
 */
export function insertionLineY(slots: Slot[], index: number, gap: number): number {
  if (slots.length === 0) return 0;
  if (index >= slots.length) {
    const last = slots[slots.length - 1];
    return last.top + last.height + gap / 2;
  }
  return slots[index].top - gap / 2;
}

/**
 * how far to scroll the list on each frame while dragging near its top
 * or bottom edge: faster the closer the pointer is, negative for up.
 */
export function autoScrollStep(pointerY: number, viewHeight: number, edge = 48, fastest = 18): number {
  if (pointerY < edge) return -Math.ceil(fastest * Math.min(1, (edge - pointerY) / edge));
  if (pointerY > viewHeight - edge) return Math.ceil(fastest * Math.min(1, (pointerY - (viewHeight - edge)) / edge));
  return 0;
}

/** a drag only starts once the pointer has moved this far, so clicks stay clicks. */
export const DRAG_THRESHOLD_PX = 5;

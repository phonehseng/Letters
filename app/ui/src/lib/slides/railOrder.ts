// the slide rail's choices, kept out of the component so they can be
// tested: where a dragged slide lands, which slides a Shift or Ctrl click
// selects, and the operations that move, delete or duplicate them.

import type { Operation } from "./operations";

/** the gap (0 is before the first slide) nearest a pointer, from each thumbnail's middle. */
export function dropGap(pointerY: number, middles: number[]): number {
  const after = middles.findIndex((middle) => pointerY < middle);
  return after === -1 ? middles.length : after;
}

/** the move a drop makes, or null when the slide would stay where it is. */
export function dropOperation(from: number, gap: number): Operation | null {
  const to = gap > from ? gap - 1 : gap;
  return to === from ? null : { op: "moveSlide", from, to };
}

/** the selection after a click: Shift extends from `anchor`, Ctrl (Command) toggles. */
export function railSelection(ids: string[], selected: string[], anchor: number, clicked: number, shift: boolean, toggle: boolean): string[] {
  if (shift) {
    const [first, last] = anchor <= clicked ? [anchor, clicked] : [clicked, anchor];
    return ids.slice(first, last + 1);
  }
  if (toggle) {
    const id = ids[clicked];
    const without = selected.filter((each) => each !== id);
    return without.length === selected.length ? [...selected, id] : without.length > 0 ? without : [id];
  }
  return [ids[clicked]];
}

/**
 * deleting several slides: from the last to the first, so each index is
 * still right when its turn comes. a deck always keeps one slide.
 */
export function deleteOperations(ids: string[], selected: string[]): Operation[] {
  const indexes = selected
    .map((id) => ids.indexOf(id))
    .filter((index) => index >= 0)
    .sort((a, b) => b - a);
  const keep = indexes.length >= ids.length ? indexes.slice(0, ids.length - 1) : indexes;
  return keep.map((index) => ({ op: "deleteSlide", index }));
}

/**
 * the layout New Slide uses, as powerpoint chooses it: after a title
 * slide, the first layout with content ("Title and Content"); otherwise the
 * current slide's own layout.
 */
export function newSlideLayout(layouts: { id: string; name: string }[], current: string | null): string | null {
  const now = layouts.find((layout) => layout.id === current);
  const isTitleSlide = now ? /title slide|^title$/i.test(now.name) : false;
  if (now && !isTitleSlide) return now.id;
  const content = layouts.find((layout) => /content|text/i.test(layout.name) && !/two|comparison/i.test(layout.name));
  return content?.id ?? layouts[1]?.id ?? layouts[0]?.id ?? null;
}

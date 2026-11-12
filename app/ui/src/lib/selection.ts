// which pages are selected in the thumbnails, with the rules every
// desktop list follows: click selects one, ctrl (command on a mac) click
// adds or removes one, shift click selects a run from the last click,
// and the arrow keys move.
//
// pages are counted from 0. a selection is a plain value; every change
// makes a new one, so the screen can't drift from the rules here.

export interface PageSelection {
  /** the selected pages, in order, each once */
  pages: number[];
  /** where a shift click run starts: the last page clicked on its own */
  anchor: number | null;
  /** the page the keyboard is on */
  focus: number | null;
}

export const NO_SELECTION: PageSelection = { pages: [], anchor: null, focus: null };

const sorted = (pages: Iterable<number>) => [...new Set(pages)].sort((a, b) => a - b);
const run = (from: number, to: number) =>
  Array.from({ length: Math.abs(to - from) + 1 }, (_, step) => Math.min(from, to) + step);

export function isSelected(selection: PageSelection, page: number): boolean {
  return selection.pages.includes(page);
}

/** a plain click: just this page. */
export function selectOnly(page: number): PageSelection {
  return { pages: [page], anchor: page, focus: page };
}

/** ctrl or command click: add the page, or take it out if it was in. */
export function toggle(selection: PageSelection, page: number): PageSelection {
  const pages = isSelected(selection, page)
    ? selection.pages.filter((other) => other !== page)
    : sorted([...selection.pages, page]);
  return { pages, anchor: page, focus: page };
}

/**
 * shift click: every page from the anchor to this one. pages added by
 * ctrl click before the anchor was set are replaced, as in Finder.
 */
export function extendTo(selection: PageSelection, page: number): PageSelection {
  const anchor = selection.anchor ?? selection.focus ?? page;
  return { pages: run(anchor, page), anchor, focus: page };
}

/** what a click does, depending on the keys held. */
export function clickPage(selection: PageSelection, page: number, keys: { toggle: boolean; extend: boolean }): PageSelection {
  if (keys.extend) return extendTo(selection, page);
  if (keys.toggle) return toggle(selection, page);
  return selectOnly(page);
}

export function selectAll(pageCount: number): PageSelection {
  if (pageCount === 0) return NO_SELECTION;
  return { pages: run(0, pageCount - 1), anchor: 0, focus: pageCount - 1 };
}

/**
 * the arrow keys, Home and End. without shift the selection moves to the
 * new page; with shift it grows or shrinks from the anchor. returns null
 * for other keys.
 */
export function moveByKey(
  selection: PageSelection,
  key: string,
  pageCount: number,
  extend: boolean,
  fallbackFocus: number,
): PageSelection | null {
  if (pageCount === 0) return null;
  const focus = selection.focus ?? fallbackFocus;
  const targets: Record<string, number> = {
    ArrowDown: focus + 1,
    ArrowRight: focus + 1,
    ArrowUp: focus - 1,
    ArrowLeft: focus - 1,
    Home: 0,
    End: pageCount - 1,
  };
  if (!(key in targets)) return null;
  const target = Math.min(Math.max(targets[key], 0), pageCount - 1);
  return extend ? extendTo({ ...selection, anchor: selection.anchor ?? focus }, target) : selectOnly(target);
}

/** drops pages past the end, for after pages were deleted. */
export function keepWithin(selection: PageSelection, pageCount: number): PageSelection {
  const inside = (page: number | null) => (page !== null && page < pageCount ? page : null);
  return {
    pages: selection.pages.filter((page) => page < pageCount),
    anchor: inside(selection.anchor),
    focus: inside(selection.focus),
  };
}

/**
 * the pages a tool works on: the selection when there is one, otherwise
 * the page being read. `fromSelection` lets the tool say which it is.
 */
export function targetPages(selection: PageSelection, currentPage: number): { pages: number[]; fromSelection: boolean } {
  if (selection.pages.length > 0) return { pages: selection.pages, fromSelection: true };
  return { pages: [currentPage], fromSelection: false };
}

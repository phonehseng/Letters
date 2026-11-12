// Back and Forward through the places a reader jumped to, like a web
// browser's: following a reference to the endnotes and coming back.
//
// only jumps are remembered (Go To, an outline entry, a bookmark, a
// search result), not every scroll, so Back goes where the reader expects.

export interface Place {
  page: number;
  /** word documents: the block at the top of the screen */
  block?: number | null;
}

export interface History {
  back: Place[];
  forward: Place[];
}

export const EMPTY_HISTORY: History = { back: [], forward: [] };

/** places beyond this many are forgotten, oldest first */
export const MAX_PLACES = 100;

function samePlace(a: Place | undefined, b: Place): boolean {
  return !!a && a.page === b.page && (a.block ?? null) === (b.block ?? null);
}

/** a jump from `from`: it can be come back to, and Forward starts again. */
export function jumped(history: History, from: Place, to: Place): History {
  if (samePlace(from, to)) return history;
  const back = samePlace(history.back.at(-1), from) ? history.back : [...history.back, from];
  return { back: back.slice(-MAX_PLACES), forward: [] };
}

/** Back from `here`: where to go and the history after it, or null when there is nowhere. */
export function goBack(history: History, here: Place): { to: Place; history: History } | null {
  const to = history.back.at(-1);
  if (!to) return null;
  return { to, history: { back: history.back.slice(0, -1), forward: [...history.forward, here] } };
}

export function goForward(history: History, here: Place): { to: Place; history: History } | null {
  const to = history.forward.at(-1);
  if (!to) return null;
  return { to, history: { back: [...history.back, here], forward: history.forward.slice(0, -1) } };
}

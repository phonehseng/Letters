// "open the document at this page with this query": a search result
// opens its document in this window or a new one, and the document's
// screen picks up where to go once it has opened.
//
// the hand-over goes through the window's local storage, which every
// window of the app shares, so it works for a new window too. it is
// kept only briefly: an arrival not taken within a minute is dropped.

export interface Arrival {
  /** the page to show, counted from 0 */
  page: number;
  /** word documents: the block to scroll to */
  block?: number | null;
  /** what to put in the document's own Find bar */
  query: string;
  at: number;
}

const PREFIX = "letters.arrival:";
const KEEP_MS = 60_000;

/** a small key/value store, so tests need no browser. */
export interface KeyValue {
  getItem(key: string): string | null;
  setItem(key: string, value: string): void;
  removeItem(key: string): void;
}

/**
 * the words for the document's own Find bar, which matches plain text:
 * quotes, OR and -excluded words belong to Search All Documents only, and
 * a corrected typo is replaced by the word that was found.
 */
export function findTextFor(query: string, corrections: { typed: string; used: string }[] = []): string {
  const words = query
    .replace(/["“”]/g, " ")
    .split(/\s+/)
    .filter((word) => word && word !== "OR" && !word.startsWith("-"))
    .map((word) => corrections.find((c) => c.typed === word.toLowerCase())?.used ?? word);
  return words.join(" ");
}

function storage(): KeyValue | null {
  try {
    return window.localStorage;
  } catch {
    return null;
  }
}

function keyFor(path: string): string {
  return PREFIX + path.replace(/\//g, "\\").toLowerCase();
}

export function setArrival(path: string, arrival: Omit<Arrival, "at">, store: KeyValue | null = storage(), now = Date.now()) {
  store?.setItem(keyFor(path), JSON.stringify({ ...arrival, at: now }));
}

/** the arrival for a file, once: it is removed when taken. */
export function takeArrival(path: string, store: KeyValue | null = storage(), now = Date.now()): Arrival | null {
  if (!store) return null;
  const key = keyFor(path);
  const stored = store.getItem(key);
  if (!stored) return null;
  store.removeItem(key);
  try {
    const arrival = JSON.parse(stored) as Arrival;
    if (now - arrival.at > KEEP_MS || !Number.isFinite(arrival.page)) return null;
    return arrival;
  } catch {
    return null;
  }
}

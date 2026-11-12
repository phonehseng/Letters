// the Font Library's search state: what the search field and the filter
// chips ask for, turned into one query for the engine, and the words the
// list shows about the answer.

import type { Category, FamilySummary, Place, SearchQuery, Source } from "./fontsApi";

/** what the chips and fields hold. every filter starts off. */
export interface FilterState {
  text: string;
  category: Category | null;
  /** a writing system id ("vietnamese"), or null for any */
  writing: string | null;
  place: Place | null;
  embeddable: boolean;
  favouritesOnly: boolean;
  /** only fonts that can draw the sample text */
  mustDrawSample: boolean;
  sample: string;
}

export const EMPTY_FILTERS: FilterState = {
  text: "",
  category: null,
  writing: null,
  place: null,
  embeddable: false,
  favouritesOnly: false,
  mustDrawSample: false,
  sample: "",
};

export const CATEGORY_LABELS: Record<Category, string> = {
  serif: "Serif",
  sansSerif: "Sans Serif",
  monospace: "Monospaced",
  display: "Display",
  handwriting: "Handwriting",
  symbol: "Symbols",
};

/** the category chips, in the order they are shown. */
export const CATEGORY_CHIPS: Category[] = ["serif", "sansSerif", "monospace", "handwriting", "display"];

export const PLACE_LABELS: Record<Place, string> = {
  onThisComputer: "On This Computer",
  addedByMe: "Added by Me",
  lettersLibrary: "Letters Library",
  toDownload: "To Download",
};

export const PLACE_CHIPS: Place[] = ["onThisComputer", "lettersLibrary", "addedByMe", "toDownload"];

/** the engine's query for the filters. */
export function queryOf(filters: FilterState): SearchQuery {
  const query: SearchQuery = { text: filters.text.trim() };
  if (filters.category) query.categories = [filters.category];
  if (filters.writing) query.writing = [filters.writing];
  if (filters.place) query.places = [filters.place];
  if (filters.embeddable) query.embeddable = true;
  if (filters.favouritesOnly) query.favouritesOnly = true;
  if (filters.mustDrawSample && filters.sample.trim()) query.sample = filters.sample.trim();
  return query;
}

/** a chip pressed again turns off; another chip of the same row replaces it. */
export function toggle<T>(current: T | null, chosen: T): T | null {
  return current === chosen ? null : chosen;
}

/** true when any filter beyond the typed text is on. */
export function hasFilters(filters: FilterState): boolean {
  return (
    filters.category !== null ||
    filters.writing !== null ||
    filters.place !== null ||
    filters.embeddable ||
    filters.favouritesOnly ||
    (filters.mustDrawSample && filters.sample.trim() !== "")
  );
}

const count = new Intl.NumberFormat("en-US");

/** the live count under the search field, also read out by screen readers. */
export function resultSentence(found: number, filters: FilterState, scanning: boolean): string {
  const fonts = found === 1 ? "1 font" : `${count.format(found)} fonts`;
  if (found === 0) {
    const typed = filters.text.trim();
    const what = typed ? `No fonts match “${typed}”.` : "No fonts match.";
    return hasFilters(filters) ? `${what} Try turning off a filter.` : `${what} Check the spelling or try fewer words.`;
  }
  return scanning ? `${fonts} so far. Still looking for fonts on this computer…` : fonts;
}

/** "2.4 MB" or "640 KB", as the download button says it. */
export function sizeWords(bytes: number): string {
  if (bytes >= 1024 * 1024) return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
  return `${Math.max(1, Math.ceil(bytes / 1024))} KB`;
}

/** where a family comes from, in two or three words. */
export function sourceWords(sources: Source[]): string {
  if (sources.includes("system")) return "On This Computer";
  if (sources.includes("custom")) return "Added by Me";
  if (sources.includes("bundled")) return "Comes with Letters";
  if (sources.includes("library")) return "Downloaded";
  return "To Download";
}

/** the quiet line under a family's name: "4 styles · Sans Serif · On This Computer". */
export function familyMeta(family: FamilySummary): string {
  const styles = family.styleCount === 1 ? "1 style" : `${family.styleCount} styles`;
  const parts = [styles, CATEGORY_LABELS[family.category], sourceWords(family.sources)];
  if (family.downloadBytes !== null && !family.onThisComputer) parts.push(sizeWords(family.downloadBytes));
  return parts.join(" · ");
}

/**
 * runs only the newest of several overlapping calls: typing fast starts
 * a search per letter, and an older answer arriving late must not
 * replace a newer one.
 */
export function latestOnly<A extends unknown[], R>(run: (...args: A) => Promise<R>) {
  let newest = 0;
  return async (...args: A): Promise<{ current: boolean; value: R }> => {
    const ticket = ++newest;
    const value = await run(...args);
    return { current: ticket === newest, value };
  };
}

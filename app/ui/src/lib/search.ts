// finding text in the open document: which match is current, what the
// count says, where the highlights go, and making sure only the answer
// to the latest query is ever shown.
//
// the searching itself happens in the engine; this file only arranges
// its answers, so all of it is unit tested.

import type { SearchMatch, SearchResult } from "./api";

/** the most matches the engine returns before it stops looking. */
export const MATCH_LIMIT = 500;

/** how long typing must pause before a search starts, in milliseconds. */
export const SEARCH_DELAY_MS = 250;

/** the next match, going round to the first after the last. */
export function nextMatch(current: number, count: number): number {
  if (count === 0) return -1;
  return (current + 1) % count;
}

/** the previous match, going round to the last before the first. */
export function previousMatch(current: number, count: number): number {
  if (count === 0) return -1;
  return (current - 1 + count) % count;
}

/**
 * the match to start on: the first one on or after the page being read,
 * so a search never throws the reader back to page 1 for no reason.
 */
export function firstMatchFrom(matches: SearchMatch[], currentPage: number): number {
  if (matches.length === 0) return -1;
  const index = matches.findIndex((match) => match.page >= currentPage);
  return index === -1 ? 0 : index;
}

/** "3 of 41", "3 of 500+", "Not found", or nothing before anything is typed. */
export function matchCountLabel(current: number, result: SearchResult | null): string {
  if (!result) return "";
  if (result.matches.length === 0) return "Not found";
  const total = result.truncated ? `${result.matches.length}+` : String(result.matches.length);
  return `${current + 1} of ${total}`;
}

/** match indexes grouped by page, so each page finds its own quickly. */
export function matchesByPage(matches: SearchMatch[]): Map<number, number[]> {
  const byPage = new Map<number, number[]>();
  matches.forEach((match, index) => {
    const list = byPage.get(match.page);
    if (list) list.push(index);
    else byPage.set(match.page, [index]);
  });
  return byPage;
}

/** one highlight rectangle in css pixels from the page's top left corner. */
export interface Highlight {
  key: string;
  left: number;
  top: number;
  width: number;
  height: number;
  current: boolean;
}

/** the highlights of one page at the given zoom (css pixels per point). */
export function highlightsOnPage(
  matches: SearchMatch[],
  byPage: Map<number, number[]>,
  page: number,
  zoom: number,
  current: number,
): Highlight[] {
  return (byPage.get(page) ?? []).flatMap((matchIndex) =>
    matches[matchIndex].rects.map((rect, rectIndex) => ({
      key: `${matchIndex}/${rectIndex}`,
      left: rect.x * zoom,
      top: rect.y * zoom,
      width: rect.width * zoom,
      height: rect.height * zoom,
      current: matchIndex === current,
    })),
  );
}

/**
 * the box around all of a match's rectangles, in points. a match that
 * wraps onto a second line has two rectangles; scrolling shows both.
 */
export function matchBounds(match: SearchMatch): { x: number; y: number; width: number; height: number } | null {
  if (match.rects.length === 0) return null;
  const left = Math.min(...match.rects.map((rect) => rect.x));
  const top = Math.min(...match.rects.map((rect) => rect.y));
  const right = Math.max(...match.rects.map((rect) => rect.x + rect.width));
  const bottom = Math.max(...match.rects.map((rect) => rect.y + rect.height));
  return { x: left, y: top, width: right - left, height: bottom - top };
}

export interface SearchRunnerOptions {
  search: (query: string) => Promise<SearchResult>;
  /** called with the answer to the latest query, or null when the query is empty */
  onResult: (query: string, result: SearchResult | null) => void;
  onError: (query: string, error: unknown) => void;
  delayMs?: number;
}

export interface SearchRunner {
  /** the query changed: search for it after a pause in typing */
  update: (query: string) => void;
  /** forget any search in progress, for example when find is closed */
  cancel: () => void;
}

/**
 * waits for a pause in typing, then searches, and throws away answers to
 * queries that have since changed. a slow search for "th" must not
 * replace the results of "the" that arrived first.
 */
export function createSearchRunner(options: SearchRunnerOptions): SearchRunner {
  const delay = options.delayMs ?? SEARCH_DELAY_MS;
  let timer: ReturnType<typeof setTimeout> | undefined;
  // every query gets a number; only answers carrying the newest one count
  let latest = 0;

  function cancel() {
    clearTimeout(timer);
    latest += 1;
  }

  async function run(query: string, ticket: number) {
    try {
      const result = await options.search(query);
      if (ticket === latest) options.onResult(query, result);
    } catch (error) {
      if (ticket === latest) options.onError(query, error);
    }
  }

  function update(query: string) {
    cancel();
    const ticket = latest;
    if (query.trim() === "") {
      options.onResult(query, null);
      return;
    }
    timer = setTimeout(() => run(query, ticket), delay);
  }

  return { update, cancel };
}

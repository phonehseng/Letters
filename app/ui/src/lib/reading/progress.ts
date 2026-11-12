// the reading progress line: "Page 41 of 300 · 14% · About 3 hours left".
//
// the time left comes from the reader's own speed on this document,
// measured on this computer while they read (see shell-core reading/store.rs).
// until there is a measurement it is left out rather than guessed.

/** "Page 41 of 300" with the printed label when it differs: "Page 41 (xli) of 300". */
export function pageOfTotal(page: number, count: number, label?: string): string {
  const number = page + 1;
  const printed = label && label !== String(number) ? ` (${label})` : "";
  return `Page ${number}${printed} of ${count}`;
}

/** how far through, in whole percent: the last page is 100%. */
export function percentThrough(page: number, count: number): number {
  if (count <= 1) return 100;
  return Math.round((page / (count - 1)) * 100);
}

/** "About 3 hours left", "About 25 minutes left", "Less than a minute left". */
export function timeLeft(page: number, count: number, secondsPerPage: number | null): string {
  if (!secondsPerPage || secondsPerPage <= 0) return "";
  const pagesLeft = Math.max(0, count - page - 1);
  if (pagesLeft === 0) return "";
  const minutes = (pagesLeft * secondsPerPage) / 60;
  if (minutes < 1) return "Less than a minute left";
  if (minutes < 55) return `About ${Math.round(minutes)} minute${Math.round(minutes) === 1 ? "" : "s"} left`;
  const hours = minutes / 60;
  if (hours < 1.5) return "About an hour left";
  return `About ${Math.round(hours)} hours left`;
}

export function progressSentence(page: number, count: number, secondsPerPage: number | null, label?: string): string {
  const parts = [pageOfTotal(page, count, label), `${percentThrough(page, count)}%`];
  const left = timeLeft(page, count, secondsPerPage);
  if (left) parts.push(left);
  return parts.join(" · ");
}

/**
 * measures time on a page while the reader reads. a page counts once the
 * reader moves to the next page after it; jumps elsewhere and long gaps
 * are not reading and are not measured.
 */
export class PageTimer {
  private page: number | null = null;
  private since = 0;

  constructor(private readonly now: () => number = () => Date.now()) {}

  /** the page in view changed. returns seconds spent on the page left, when it was read through. */
  moved(page: number): number | null {
    const at = this.now();
    const previous = this.page;
    const seconds = (at - this.since) / 1000;
    this.page = page;
    this.since = at;
    if (previous === null || page !== previous + 1) return null;
    return seconds;
  }

  /** the window was left or the reader paused: the page's time starts again. */
  restart() {
    this.since = this.now();
  }
}

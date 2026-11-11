// small helpers that turn numbers into friendly words.

const MINUTE = 60;
const HOUR = 60 * MINUTE;
const DAY = 24 * HOUR;

/** "just now", "5 minutes ago", "yesterday", "3 days ago", or a date. */
export function timeAgo(thenSeconds: number, nowSeconds: number): string {
  const elapsed = Math.max(0, nowSeconds - thenSeconds);
  if (elapsed < MINUTE) return "just now";
  if (elapsed < HOUR) return plural(Math.floor(elapsed / MINUTE), "minute") + " ago";
  if (elapsed < DAY) return plural(Math.floor(elapsed / HOUR), "hour") + " ago";
  if (elapsed < 2 * DAY) return "yesterday";
  if (elapsed < 30 * DAY) return plural(Math.floor(elapsed / DAY), "day") + " ago";
  return new Date(thenSeconds * 1000).toLocaleDateString();
}

export function plural(count: number, word: string): string {
  return `${count} ${word}${count === 1 ? "" : "s"}`;
}

/** true when every word typed in the search box appears in the text. */
export function matchesSearch(text: string, search: string): boolean {
  const haystack = text.toLowerCase();
  return search
    .toLowerCase()
    .split(/\s+/)
    .filter((word) => word.length > 0)
    .every((word) => haystack.includes(word));
}

// moving through a list of controls with the arrow keys, as menus, tabs
// and lists do on every desktop system. one helper for all of them, so
// they all behave the same.

export interface ArrowOptions {
  /** which arrows move: up and down for lists and menus, left and right for tabs */
  orientation: "vertical" | "horizontal";
  /** whether moving past the end comes back at the start (menus and tabs do) */
  wrap: boolean;
}

/**
 * the index the focus moves to after a key, or null when the key is not
 * a moving key (so the caller leaves the event alone).
 */
export function nextIndex(current: number, key: string, count: number, options: ArrowOptions): number | null {
  if (count === 0) return null;
  const back = options.orientation === "vertical" ? "ArrowUp" : "ArrowLeft";
  const forward = options.orientation === "vertical" ? "ArrowDown" : "ArrowRight";
  if (key === "Home") return 0;
  if (key === "End") return count - 1;
  if (key !== back && key !== forward) return null;

  const step = key === forward ? 1 : -1;
  const moved = current + step;
  if (options.wrap) return (moved + count) % count;
  return Math.min(Math.max(moved, 0), count - 1);
}

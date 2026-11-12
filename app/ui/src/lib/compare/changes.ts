// the changes of a comparison as the Compare screen lists and draws them:
// grouped by page, filtered by category, stepped through one at a time,
// and worded. nothing here touches the screen, so all of it is tested.

import type { Category, Change, ChangeKind, ComparisonResult, PagePair, Summary } from "./compareApi";

/** the filters in the changes list, in the order they are shown. */
export const FILTERS: { category: Category; label: string }[] = [
  { category: "text", label: "Text" },
  { category: "formatting", label: "Formatting" },
  { category: "layout", label: "Layout" },
  { category: "image", label: "Pictures" },
  { category: "page", label: "Pages" },
];

/** how a change is drawn: its tone (a quiet colour) and its small label. */
export type Tone = "removed" | "added" | "moved" | "formatting" | "layout" | "picture";

export interface MarkStyle {
  tone: Tone;
  /** the word on the mark itself, so colour is never the only sign */
  label: string;
}

/** the side of the comparison a rectangle is on. */
export type Side = "old" | "new";

const KIND_WORDS: Record<ChangeKind, string> = {
  inserted: "Added",
  deleted: "Removed",
  replaced: "Changed",
  moved: "Moved",
  restyled: "Formatting",
  shifted: "Layout",
  reflowed: "Layout",
  resized: "Page size",
  redrawn: "Picture",
  pageAdded: "Page added",
  pageRemoved: "Page removed",
  pageMoved: "Page moved",
};

/** the word for a kind of change, as the list and the marks say it. */
export function kindLabel(kind: ChangeKind): string {
  return KIND_WORDS[kind];
}

/**
 * how a change looks on one side: text that went is red-ish on the old
 * side, text that came is green-ish on the new side, the rest have quiet
 * colours of their own. a replaced word is "removed" on the left and
 * "added" on the right, as a reader expects.
 */
export function markStyle(kind: ChangeKind, side: Side): MarkStyle {
  const label = kindLabel(kind);
  switch (kind) {
    case "inserted":
    case "pageAdded":
      return { tone: "added", label };
    case "deleted":
    case "pageRemoved":
      return { tone: "removed", label };
    case "replaced":
      return { tone: side === "old" ? "removed" : "added", label };
    case "moved":
    case "pageMoved":
      return { tone: "moved", label };
    case "restyled":
      return { tone: "formatting", label };
    case "redrawn":
      return { tone: "picture", label };
    default:
      return { tone: "layout", label };
  }
}

/** one row of the changes list: a page heading, or a change under it. */
export type ListRow =
  | { type: "heading"; pair: number; text: string; count: number }
  | { type: "change"; pair: number; change: Change };

/** "Page 3", or "Old page 2, new page 3" when the numbers differ. */
export function pairHeading(pair: PagePair): string {
  const { leftPage: old, rightPage: next } = pair;
  if (old === null && next !== null) return `New page ${next + 1}`;
  if (next === null && old !== null) return `Old page ${old + 1}`;
  if (old === next && old !== null) return `Page ${old + 1}`;
  return `Old page ${(old ?? 0) + 1}, new page ${(next ?? 0) + 1}`;
}

/** true when the change belongs to one of the shown categories. */
export function isShown(change: Change, shown: ReadonlySet<Category>): boolean {
  return shown.has(change.category);
}

/** the list's rows: each page pair with changes, its heading, then its changes. */
export function listRows(result: ComparisonResult, shown: ReadonlySet<Category>): ListRow[] {
  const rows: ListRow[] = [];
  result.pagePairs.forEach((pair, index) => {
    const changes = pair.changes.filter((change) => isShown(change, shown));
    if (changes.length === 0) return;
    rows.push({ type: "heading", pair: index, text: pairHeading(pair), count: changes.length });
    for (const change of changes) rows.push({ type: "change", pair: index, change });
  });
  return rows;
}

/** every shown change in reading order, with the pair it belongs to. */
export function orderedChanges(result: ComparisonResult, shown: ReadonlySet<Category>): { pair: number; change: Change }[] {
  return result.pagePairs.flatMap((pair, index) =>
    pair.changes.filter((change) => isShown(change, shown)).map((change) => ({ pair: index, change })),
  );
}

/**
 * the change after (or before) the selected one. with nothing selected,
 * Next goes to the first and Previous to the last; at the ends it stays.
 */
export function stepChange(
  ordered: { change: { id: number } }[],
  selectedId: number | null,
  direction: "next" | "previous",
): number | null {
  if (ordered.length === 0) return null;
  const index = ordered.findIndex((entry) => entry.change.id === selectedId);
  if (index < 0) return direction === "next" ? ordered[0].change.id : ordered[ordered.length - 1].change.id;
  const target = direction === "next" ? Math.min(index + 1, ordered.length - 1) : Math.max(index - 1, 0);
  return ordered[target].change.id;
}

/** "Change 3 of 14", for the live region and the buttons. */
export function positionText(ordered: { change: { id: number } }[], selectedId: number | null): string {
  const index = ordered.findIndex((entry) => entry.change.id === selectedId);
  if (index < 0) return "";
  return `Change ${index + 1} of ${ordered.length}`;
}

/** finds a change by id, with the pair it belongs to. */
export function findChange(result: ComparisonResult, id: number | null): { pair: number; change: Change } | null {
  if (id === null) return null;
  for (let index = 0; index < result.pagePairs.length; index += 1) {
    const change = result.pagePairs[index].changes.find((each) => each.id === id);
    if (change) return { pair: index, change };
  }
  return null;
}

const SUMMARY_PARTS: { key: keyof Summary; word: string }[] = [
  { key: "text", word: "text" },
  { key: "formatting", word: "formatting" },
  { key: "layout", word: "layout" },
  { key: "image", word: "pictures" },
  { key: "page", word: "pages" },
];

/** "14 changes: 9 text, 3 formatting, 2 layout", or the sentence for no changes. */
export function summaryLine(summary: Summary): string {
  if (summary.total === 0) return "These two documents are the same.";
  const parts = SUMMARY_PARTS.filter((part) => summary[part.key] > 0).map((part) => `${summary[part.key]} ${part.word}`);
  const noun = summary.total === 1 ? "change" : "changes";
  return `${summary.total} ${noun}: ${parts.join(", ")}`;
}

/** what was left out on purpose, or null when nothing was. */
export function ignoredLine(summary: Summary): string | null {
  if (summary.ignored === 0) return null;
  const what = summary.ignored === 1 ? "difference" : "differences";
  return `${summary.ignored} ${what} only in spacing or hyphenation ${summary.ignored === 1 ? "was" : "were"} left out.`;
}

/** what screen readers hear about a selected change: where, what kind, the sentence. */
export function changeAnnouncement(ordered: { change: Change }[], change: Change, pair: PagePair): string {
  return `${positionText(ordered, change.id)}. ${pairHeading(pair)}. ${kindLabel(change.kind)}: ${change.detail}`;
}

/**
 * the shown changes drawn on each page of one side, by page number. a
 * moved passage is drawn on the page it came from (old side) and the
 * page it went to (new side), which may belong to other pairs.
 */
export function changesByPage(result: ComparisonResult, side: Side, shown: ReadonlySet<Category>): Map<number, Change[]> {
  const byPage = new Map<number, Change[]>();
  for (const pair of result.pagePairs) {
    for (const change of pair.changes) {
      if (!isShown(change, shown)) continue;
      const page = side === "old" ? change.leftPage : change.rightPage;
      const rects = side === "old" ? change.leftRects : change.rightRects;
      // a page change without rectangles is about the whole page, which is drawn as one mark
      if (page === null || (rects.length === 0 && change.category !== "page")) continue;
      const list = byPage.get(page) ?? [];
      list.push(change);
      byPage.set(page, list);
    }
  }
  return byPage;
}

/** the rectangles of a change on one page of one side. */
export function rectsOn(change: Change, side: Side, page: number): Change["leftRects"] {
  if (side === "old") return change.leftPage === page ? change.leftRects : [];
  return change.rightPage === page ? change.rightRects : [];
}

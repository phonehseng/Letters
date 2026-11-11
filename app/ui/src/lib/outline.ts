// the document's outline (its bookmarks) as the sidebar shows it.

import type { OutlineItem } from "./api";

/**
 * the entry for the part being read: the last entry whose page is at or
 * before the current page, or -1 before the first entry starts.
 */
export function currentOutlineIndex(items: OutlineItem[], currentPage: number): number {
  let found = -1;
  items.forEach((item, index) => {
    if (item.page <= currentPage) found = index;
  });
  return found;
}

/** what a screen reader needs to say "level 2, 3 of 5" for each entry. */
export interface TreePosition {
  level: number;
  positionInSet: number;
  setSize: number;
}

/**
 * the level, position and number of siblings of every entry, worked out
 * from the flat reading-order list the engine gives.
 *
 * siblings are entries at the same level with the same parent; a jump
 * of more than one level (a broken outline) is treated as one level.
 */
export function treePositions(items: OutlineItem[]): TreePosition[] {
  const levels = normalisedLevels(items);
  const positions: TreePosition[] = levels.map((level) => ({ level: level + 1, positionInSet: 0, setSize: 0 }));
  // for each level, the entries of the sibling group currently open at it
  const openGroups: number[][] = [];
  levels.forEach((level, index) => {
    openGroups.length = level + 1;
    const group = (openGroups[level] ??= []);
    group.push(index);
    positions[index].positionInSet = group.length;
  });
  // a second pass fills in the sizes, now that every group is complete
  countSiblings(levels).forEach((size, index) => (positions[index].setSize = size));
  return positions;
}

/** levels with no gaps: a child is never more than one level below its parent. */
function normalisedLevels(items: OutlineItem[]): number[] {
  const levels: number[] = [];
  items.forEach((item, index) => {
    const previous = index === 0 ? -1 : levels[index - 1];
    levels.push(Math.max(0, Math.min(item.level, previous + 1)));
  });
  return levels;
}

function countSiblings(levels: number[]): number[] {
  const sizes = new Array<number>(levels.length).fill(0);
  const openGroups: number[][] = [];
  const finish = (group: number[] | undefined) => group?.forEach((index) => (sizes[index] = group.length));
  levels.forEach((level, index) => {
    // deeper groups end when a shallower entry appears
    openGroups.splice(level + 1).forEach(finish);
    (openGroups[level] ??= []).push(index);
  });
  openGroups.forEach(finish);
  return sizes;
}

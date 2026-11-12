// finding the text and pictures of a page under the pointer, and finding
// them again after an edit.
//
// the engine's ids change with every edit of a page, so something the
// person was working on is found again by where it is: the block or
// picture that overlaps its old place the most.

import { rectContains, type Point, type Rect } from "../pageTools/pagePoints";
import type { ImageObject, PageObjects, TextBlock, TextLine } from "./editApi";

/** blocks a person can edit: recognized text over a scan is invisible and edited in the Scan tab instead. */
export function visibleBlocks(objects: PageObjects): TextBlock[] {
  return objects.textBlocks.filter((block) => block.lines.some((line) => line.runs.some((run) => !run.invisible)));
}

function area(rect: Rect): number {
  return Math.max(0, rect.width) * Math.max(0, rect.height);
}

/** the block under a point, with `slack` points to spare; the smallest wins where blocks overlap. */
export function blockAt(objects: PageObjects, point: Point, slack = 2): TextBlock | null {
  let best: TextBlock | null = null;
  for (const block of visibleBlocks(objects)) {
    if (!rectContains(block.rect, point, slack)) continue;
    if (!best || area(block.rect) < area(best.rect)) best = block;
  }
  return best;
}

/** the line of a block nearest a point, by height. */
export function lineAt(block: TextBlock, point: Point): TextLine | null {
  let best: TextLine | null = null;
  let bestDistance = Infinity;
  for (const line of block.lines) {
    const top = line.rect.y;
    const bottom = line.rect.y + line.rect.height;
    const distance = point.y < top ? top - point.y : point.y > bottom ? point.y - bottom : 0;
    if (distance < bestDistance) {
      best = line;
      bestDistance = distance;
    }
  }
  return best;
}

/** the picture under a point. pictures later in the list are drawn on top, so the last match wins. */
export function imageAt(objects: PageObjects, point: Point): ImageObject | null {
  let found: ImageObject | null = null;
  for (const image of objects.images) if (rectContains(image.rect, point)) found = image;
  return found;
}

/**
 * where the caret goes in a block's text for a click: the line clicked,
 * and the character nearest across it, from that line's own width.
 * counts in the block's text, whose lines are joined with new lines.
 */
export function caretIndexFor(block: TextBlock, point: Point): number {
  const line = lineAt(block, point);
  if (!line) return block.text.length;
  let offset = 0;
  for (const each of block.lines) {
    if (each === line) break;
    offset += each.text.length + 1;
  }
  const width = Math.max(line.rect.width, 1);
  const share = Math.min(Math.max((point.x - line.rect.x) / width, 0), 1);
  return offset + Math.round(share * line.text.length);
}

function overlap(a: Rect, b: Rect): number {
  const width = Math.min(a.x + a.width, b.x + b.width) - Math.max(a.x, b.x);
  const height = Math.min(a.y + a.height, b.y + b.height) - Math.max(a.y, b.y);
  return width > 0 && height > 0 ? width * height : 0;
}

function bestMatch<T extends { rect: Rect }>(items: T[], rect: Rect): T | null {
  let best: T | null = null;
  let bestScore = -Infinity;
  for (const item of items) {
    const shared = overlap(item.rect, rect);
    const distance = Math.hypot(item.rect.x - rect.x, item.rect.y - rect.y);
    // a box that shrank to nothing still counts if it starts where the old one did
    if (shared === 0 && distance > 2) continue;
    const score = shared - distance;
    if (score > bestScore) {
      best = item;
      bestScore = score;
    }
  }
  return best;
}

/** the block that is most where a block was, after the page changed. */
export function matchingBlock(objects: PageObjects, rect: Rect): TextBlock | null {
  return bestMatch(visibleBlocks(objects), rect);
}

/** the picture that is most where a picture was. */
export function matchingImage(objects: PageObjects, rect: Rect): ImageObject | null {
  return bestMatch(objects.images, rect);
}

/** an upright picture can be resized with the frame; a turned or slanted one only moved. */
export function isUpright(image: ImageObject): boolean {
  const [topLeft, topRight, bottomLeft] = image.quad;
  return Math.abs(topLeft.y - topRight.y) < 0.5 && Math.abs(topLeft.x - bottomLeft.x) < 0.5 && topRight.x > topLeft.x;
}

/** the size a block's text is set in: its largest run, which is what the editor shows. */
export function blockFontSize(block: TextBlock): number {
  let size = 0;
  for (const line of block.lines) for (const run of line.runs) size = Math.max(size, run.size);
  return size || 12;
}

/** the first visible run of a block, whose font and colour the editor uses. */
export function leadingRun(block: TextBlock) {
  for (const line of block.lines) for (const run of line.runs) if (!run.invisible && run.text.trim()) return run;
  return block.lines[0]?.runs[0] ?? null;
}

// where the on-page editor goes, and how big its text is, so it sits
// exactly over the text being edited and nothing jumps when typing starts.
//
// the page is drawn in points; the screen is css pixels, `scale` per
// point. a css line box puts the baseline at
//   top + (line height - (ascent + descent) × font size) / 2 + ascent × font size
// so the editor's top is worked back from where the first baseline is on
// the page. ascent and descent are per em, measured from the font the
// editor really uses (see fontMetrics in cssFonts.ts).

import type { Point } from "../pageTools/pagePoints";
import { blockFontSize } from "./blocks";
import type { TextBlock } from "./editApi";

export interface FontMetrics {
  /** above the baseline, as a share of the font size */
  ascent: number;
  /** below the baseline, as a share of the font size (positive) */
  descent: number;
}

/** what most text fonts measure, for when the real numbers can't be read. */
export const TYPICAL_METRICS: FontMetrics = { ascent: 0.9, descent: 0.22 };

/** the engine's line spacing for a single line, as a share of its size. */
export const SINGLE_LINE_SPACING = 1.2;

export interface EditorBox {
  /** css pixels in the scroll content */
  left: number;
  top: number;
  width: number;
  height: number;
  /** css pixels */
  fontSize: number;
  lineHeight: number;
  /** degrees clockwise, turning around the first baseline's start */
  rotation: number;
  /** where the turn is centred, from the editor's own top left, css pixels */
  origin: Point;
}

/** the distance between the block's baselines in points, or 1.2 sizes for one line. */
export function lineSpacing(block: TextBlock): number {
  const baselines = block.lines.map((line) => line.baseline[0]);
  if (baselines.length < 2) return blockFontSize(block) * SINGLE_LINE_SPACING;
  let total = 0;
  for (let index = 1; index < baselines.length; index += 1) {
    total += Math.hypot(baselines[index].x - baselines[index - 1].x, baselines[index].y - baselines[index - 1].y);
  }
  return total / (baselines.length - 1);
}

/** how far below a line box's top its baseline sits, in css pixels. */
export function baselineOffset(lineHeight: number, fontSize: number, metrics: FontMetrics): number {
  const content = (metrics.ascent + metrics.descent) * fontSize;
  return (lineHeight - content) / 2 + metrics.ascent * fontSize;
}

/**
 * the editor over a block on a page drawn at `frame` (its place in the
 * scroll content and css pixels per point). `lines` is how many lines
 * the editor holds now, so it grows as lines are typed.
 */
export function editorBox(
  block: TextBlock,
  frame: { left: number; top: number; scale: number },
  metrics: FontMetrics,
  lines = block.lines.length,
): EditorBox {
  const scale = frame.scale;
  const fontSize = blockFontSize(block) * scale;
  const lineHeight = lineSpacing(block) * scale;
  const first = block.lines[0]?.baseline[0] ?? { x: block.rect.x, y: block.rect.y + block.rect.height };
  const turned = Math.abs(block.rotation) > 0.01;
  const baselineX = frame.left + (turned ? first.x : block.rect.x) * scale;
  const baselineY = frame.top + first.y * scale;
  const top = baselineY - baselineOffset(lineHeight, fontSize, metrics);
  // a turned block keeps its own width along its baseline
  const width = turned ? alongWidth(block) * scale : block.rect.width * scale;
  return {
    left: baselineX,
    top,
    // a little room so the last letter isn't clipped while typing
    width: width + fontSize * 0.5,
    height: Math.max(1, lines) * lineHeight,
    fontSize,
    lineHeight,
    rotation: block.rotation,
    origin: { x: 0, y: baselineY - top },
  };
}

/** how long the block's longest line runs along its baseline, in points. */
function alongWidth(block: TextBlock): number {
  let widest = 0;
  for (const line of block.lines) {
    const [start, end] = line.baseline;
    widest = Math.max(widest, Math.hypot(end.x - start.x, end.y - start.y));
  }
  return widest || block.rect.width;
}

/** where new text starts for a click: its top left corner, as the engine places it. */
export function newTextBox(point: Point, size: number, frame: { left: number; top: number; scale: number }, metrics: FontMetrics) {
  const scale = frame.scale;
  const fontSize = size * scale;
  const lineHeight = size * SINGLE_LINE_SPACING * scale;
  // the engine puts the first baseline at the top plus the font's ascent
  const baselineY = frame.top + point.y * scale + metrics.ascent * fontSize;
  return {
    left: frame.left + point.x * scale,
    top: baselineY - baselineOffset(lineHeight, fontSize, metrics),
    fontSize,
    lineHeight,
  };
}

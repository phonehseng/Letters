// the ruler above the page: where its three markers sit, what dragging
// one does to the paragraph's indents, tab stops, and the tick marks.
//
// the three markers, as people see them:
// - First Line: where the first line of the paragraph starts
// - Left: where the other lines start (drag it right of First Line for a
//   hanging indent, as in a reference list)
// - Right: where lines end
// every position here is in twips from the left edge of the page.

import type { Indent, TabStop } from "./model";
import { firstLineOffset } from "./styles";
import { rulerStep, TWIPS_PER_CM, TWIPS_PER_INCH, type LengthUnit } from "./units";

export type Marker = "firstLine" | "left" | "right";

export const MARKER_LABELS: Record<Marker, string> = { firstLine: "First Line", left: "Left", right: "Right" };

export interface RulerFrame {
  pageWidth: number;
  marginLeft: number;
  marginRight: number;
}

export interface MarkerPositions {
  firstLine: number;
  left: number;
  right: number;
}

/** lines need at least this much room between the left and right markers. */
const MIN_TEXT_WIDTH = 720;

export function markerPositions(frame: RulerFrame, indent: Indent): MarkerPositions {
  const left = frame.marginLeft + (indent.left ?? 0);
  return {
    left,
    firstLine: left + firstLineOffset(indent),
    right: frame.pageWidth - frame.marginRight - (indent.right ?? 0),
  };
}

/** a left indent and first line offset as the model stores them: one of first line or hanging. */
export function indentFrom(left: number, offset: number, right: number | undefined): Indent {
  const indent: Indent = { left };
  if (offset > 0) indent.firstLine = offset;
  else if (offset < 0) indent.hanging = -offset;
  else indent.firstLine = 0;
  if (right !== undefined) indent.right = right;
  return indent;
}

/** the indent after moving one marker to `position`, kept inside the page. */
export function moveMarker(frame: RulerFrame, indent: Indent, marker: Marker, position: number): Indent {
  const now = markerPositions(frame, indent);
  const pageLeft = 0;
  const pageRight = frame.pageWidth;
  if (marker === "right") {
    const clamped = clamp(position, Math.max(now.left, now.firstLine) + MIN_TEXT_WIDTH, pageRight);
    return indentFrom(now.left - frame.marginLeft, now.firstLine - now.left, frame.pageWidth - frame.marginRight - clamped);
  }
  const limit = now.right - MIN_TEXT_WIDTH;
  if (marker === "firstLine") {
    const clamped = clamp(position, pageLeft, limit);
    return indentFrom(now.left - frame.marginLeft, clamped - now.left, indent.right);
  }
  // Left moves the other lines; the first line stays where it is
  const clamped = clamp(position, pageLeft, limit);
  return indentFrom(clamped - frame.marginLeft, now.firstLine - clamped, indent.right);
}

function clamp(value: number, low: number, high: number): number {
  return Math.min(Math.max(value, low), Math.max(low, high));
}

/** snaps a dragged position to the ruler's small steps (1/16 inch or 1 mm). */
export function snap(position: number, unit: LengthUnit): number {
  const step = unit === "in" ? TWIPS_PER_INCH / 16 : TWIPS_PER_CM / 10;
  return Math.round(Math.round(position / step) * step);
}

/** the arrow keys move a marker by a small step, Shift by a big one. */
export function nudge(frame: RulerFrame, indent: Indent, marker: Marker, direction: -1 | 1, unit: LengthUnit, large: boolean): Indent {
  const positions = markerPositions(frame, indent);
  return moveMarker(frame, indent, marker, positions[marker] + direction * rulerStep(unit, large));
}

// --- tab stops (positions from the left margin, as the model keeps them)

export function addTabStop(tabs: TabStop[], position: number): TabStop[] {
  const kept = tabs.filter((stop) => Math.abs(stop.position - position) > 60);
  return [...kept, { position: Math.max(0, Math.round(position)), kind: "left" as const }].sort((a, b) => a.position - b.position);
}

export function removeTabStop(tabs: TabStop[], position: number): TabStop[] {
  return tabs.filter((stop) => stop.position !== position);
}

export function moveTabStop(tabs: TabStop[], from: number, to: number): TabStop[] {
  const moving = tabs.find((stop) => stop.position === from);
  if (!moving) return tabs;
  return [...tabs.filter((stop) => stop !== moving), { ...moving, position: Math.max(0, Math.round(to)) }].sort((a, b) => a.position - b.position);
}

// --- tick marks

export interface Tick {
  /** twips from the left margin (negative inside the left margin) */
  at: number;
  kind: "number" | "half" | "small";
  label?: string;
}

/** ticks across the whole page: numbers every inch or centimetre, measured from the left margin. */
export function ticks(frame: RulerFrame, unit: LengthUnit): Tick[] {
  const major = unit === "in" ? TWIPS_PER_INCH : TWIPS_PER_CM;
  const minorPerMajor = unit === "in" ? 8 : 2;
  const step = major / minorPerMajor;
  const out: Tick[] = [];
  const first = Math.ceil(-frame.marginLeft / step);
  const last = Math.floor((frame.pageWidth - frame.marginLeft) / step);
  for (let i = first; i <= last; i += 1) {
    const at = i * step;
    if (i % minorPerMajor === 0) out.push({ at, kind: "number", label: i === 0 ? "" : String(Math.abs(i / minorPerMajor)) });
    else if (unit === "in" && i % (minorPerMajor / 2) === 0) out.push({ at, kind: "half" });
    else out.push({ at, kind: "small" });
  }
  return out;
}

/** a marker's tooltip and screen reader text, in plain words. */
export function markerDescription(marker: Marker, frame: RulerFrame, indent: Indent, format: (twips: number) => string): string {
  const positions = markerPositions(frame, indent);
  if (marker === "firstLine") {
    const offset = positions.firstLine - positions.left;
    if (offset === 0) return `First Line: the first line starts where the others do, ${format(positions.firstLine - frame.marginLeft)} from the margin. Drag it to indent the first line.`;
    return offset > 0
      ? `First Line: the first line is indented ${format(offset)}. Drag it back to the Left marker to remove the indent.`
      : `First Line: the first line hangs out ${format(-offset)} to the left of the others, as in a reference list.`;
  }
  if (marker === "left") return `Left: lines after the first start ${format(positions.left - frame.marginLeft)} from the left margin. Drag it right of First Line for a hanging indent.`;
  return `Right: lines end ${format(frame.pageWidth - frame.marginRight - positions.right)} before the right margin.`;
}

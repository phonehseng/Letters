// the rules about annotations on screen: which one a click lands on,
// what its frame lets the person do, which style controls apply to it,
// and the words used for each kind.
//
// nothing here touches the screen, so all of it is unit tested.

import type { Point, Rect } from "../pageTools/pagePoints";
import type { AnnotationInfo, AnnotationKind } from "./annotateApi";

/** how close, in points, a click must be to a line or a stroke to pick it. */
export const PICK_SLACK = 4;

const KIND_LABELS: Record<AnnotationKind, string> = {
  highlight: "Highlight",
  underline: "Underline",
  strikeOut: "Strike through",
  squiggly: "Squiggly underline",
  note: "Note",
  textBox: "Text box",
  square: "Rectangle",
  circle: "Ellipse",
  line: "Line",
  polygon: "Polygon",
  polyLine: "Lines",
  ink: "Drawing",
  stamp: "Picture",
  other: "Annotation",
};

/** "Highlight", "Text box", ... a line or an arrow is told apart by its ends. */
export function kindLabel(annotation: Pick<AnnotationInfo, "kind" | "details">): string {
  const { details } = annotation;
  if (details.type === "line" && (details.endEnding !== "none" || details.startEnding !== "none")) return "Arrow";
  if (details.type === "stamp" && !details.isPicture) return "Stamp";
  return KIND_LABELS[annotation.kind];
}

/** the live announcement after something was added: "Highlight added to page 3." */
export function addedAnnouncement(label: string, page: number): string {
  return `${label} added to page ${page + 1}.`;
}

export function isTextMarkup(kind: AnnotationKind): boolean {
  return kind === "highlight" || kind === "underline" || kind === "strikeOut" || kind === "squiggly";
}

function distanceToSegment(point: Point, a: Point, b: Point): number {
  const dx = b.x - a.x;
  const dy = b.y - a.y;
  const length = dx * dx + dy * dy;
  const t = length === 0 ? 0 : Math.max(0, Math.min(1, ((point.x - a.x) * dx + (point.y - a.y) * dy) / length));
  return Math.hypot(point.x - (a.x + t * dx), point.y - (a.y + t * dy));
}

function nearPolyline(points: Point[], point: Point, reach: number): boolean {
  if (points.length === 1) return Math.hypot(point.x - points[0].x, point.y - points[0].y) <= reach;
  for (let index = 1; index < points.length; index += 1) {
    if (distanceToSegment(point, points[index - 1], points[index]) <= reach) return true;
  }
  return false;
}

function inside(rect: Rect, point: Point, slack: number): boolean {
  return (
    point.x >= rect.x - slack &&
    point.x <= rect.x + rect.width + slack &&
    point.y >= rect.y - slack &&
    point.y <= rect.y + rect.height + slack
  );
}

/**
 * whether a click at `point` lands on an annotation. lines and freehand
 * strokes are picked near their strokes only, so a big drawing doesn't
 * cover the text inside its box.
 */
export function hits(annotation: AnnotationInfo, point: Point, slack = PICK_SLACK): boolean {
  const { details } = annotation;
  if (details.type === "line") {
    return nearPolyline([details.start, details.end], point, details.strokeWidth / 2 + slack);
  }
  if (details.type === "ink") {
    return details.strokes.some((stroke) => nearPolyline(stroke, point, details.width / 2 + slack));
  }
  if (details.type === "polygon" && !details.closed) {
    return nearPolyline(details.points, point, details.strokeWidth / 2 + slack);
  }
  return inside(annotation.rect, point, details.type === "textMarkup" ? 0 : slack / 2);
}

/** the annotation a click lands on: the one drawn last (on top) wins. replies and hidden ones are skipped. */
export function annotationAt(annotations: AnnotationInfo[], page: number, point: Point): AnnotationInfo | null {
  for (let index = annotations.length - 1; index >= 0; index -= 1) {
    const annotation = annotations[index];
    if (annotation.page !== page || annotation.hidden || annotation.replyTo) continue;
    if (hits(annotation, point)) return annotation;
  }
  return null;
}

/** what the frame around a selected annotation allows. */
export interface FrameBehaviour {
  movable: boolean;
  resizable: boolean;
  keepAspect: boolean;
}

export function frameBehaviour(annotation: AnnotationInfo): FrameBehaviour {
  if (!annotation.editable) return { movable: false, resizable: false, keepAspect: false };
  // marks belong to the words under them; a note is an icon of fixed size
  if (isTextMarkup(annotation.kind)) return { movable: false, resizable: false, keepAspect: false };
  if (annotation.kind === "note") return { movable: true, resizable: false, keepAspect: false };
  const picture = annotation.details.type === "stamp";
  return { movable: true, resizable: true, keepAspect: picture };
}

/** which palette a kind's main colour comes from. */
export type ColorRole = "highlight" | "ink" | "note" | "none";

/** the style controls that apply to a selected annotation. */
export interface StyleControls {
  color: ColorRole;
  strokeWidth: boolean;
  fill: boolean;
  opacity: boolean;
  text: boolean;
}

export function styleControls(annotation: AnnotationInfo): StyleControls {
  const none: StyleControls = { color: "none", strokeWidth: false, fill: false, opacity: false, text: false };
  if (!annotation.editable) return none;
  switch (annotation.details.type) {
    case "textMarkup":
      return { ...none, color: annotation.kind === "highlight" ? "highlight" : "ink", opacity: true };
    case "note":
      return { ...none, color: "note" };
    case "textBox":
      return { ...none, color: "ink", fill: true, opacity: true, text: true };
    case "shape":
    case "polygon":
      return { ...none, color: "ink", strokeWidth: true, fill: true, opacity: true };
    case "line":
    case "ink":
      return { ...none, color: "ink", strokeWidth: true, opacity: true };
    case "stamp":
      return { ...none, opacity: true };
    default:
      return none;
  }
}

/** the line or stroke width of an annotation, when it has one. */
export function strokeWidthOf(annotation: AnnotationInfo): number | null {
  const { details } = annotation;
  if (details.type === "ink") return details.width;
  if (details.type === "shape" || details.type === "line" || details.type === "polygon") return details.strokeWidth;
  return null;
}

/** an annotation's fill, for the fill control. */
export function fillOf(annotation: AnnotationInfo): { r: number; g: number; b: number } | null {
  const { details } = annotation;
  if (details.type === "shape" || details.type === "polygon" || details.type === "textBox" || details.type === "line") {
    return details.fill;
  }
  return null;
}

/** where a note's pop-up goes: beside its icon, flipped inwards near the page's right edge. */
export function popoverSpot(icon: Rect, page: { width: number; height: number }, popoverWidth: number): Point {
  const right = icon.x + icon.width + 6;
  const x = right + popoverWidth <= page.width ? right : Math.max(0, icon.x - popoverWidth - 6);
  return { x, y: Math.max(0, Math.min(icon.y, page.height - 40)) };
}

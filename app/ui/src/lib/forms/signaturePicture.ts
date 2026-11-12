// the arithmetic behind a picture of a signature: smoothing a drawn
// stroke, the size of a typed one, making a photo's white paper see-
// through, and fitting the picture onto a page. the components draw;
// nothing here touches the screen, so all of it is unit tested.
//
// a picture of a signature shows a signature but proves nothing about
// who placed it. Sign with a Certificate is the kind that can be checked.

import type { PageSize } from "../layout";
import { keepRectOnPage } from "../pageTools/gestures";
import type { Point, Rect } from "../pageTools/pagePoints";

// ---- drawn ----

/**
 * a smooth path through a stroke's points: straight to the first
 * midpoint, then quadratic curves through each point to the next
 * midpoint. the curve passes near every point without the corners a
 * mouse leaves, which is what makes a drawn signature look written.
 */
export function smoothPath(points: Point[]): string {
  if (points.length === 0) return "";
  const [first] = points;
  if (points.length === 1) return `M${first.x} ${first.y}l0.01 0.01`;
  if (points.length === 2) return `M${first.x} ${first.y}L${points[1].x} ${points[1].y}`;
  const mid = (a: Point, b: Point) => ({ x: (a.x + b.x) / 2, y: (a.y + b.y) / 2 });
  let path = `M${first.x} ${first.y}`;
  const start = mid(points[0], points[1]);
  path += `L${start.x} ${start.y}`;
  for (let index = 1; index < points.length - 1; index += 1) {
    const end = mid(points[index], points[index + 1]);
    path += `Q${points[index].x} ${points[index].y} ${end.x} ${end.y}`;
  }
  const last = points[points.length - 1];
  return `${path}L${last.x} ${last.y}`;
}

/** the box around every stroke, grown by half the pen's width; null when nothing is drawn. */
export function strokesBounds(strokes: Point[][], penWidth: number): Rect | null {
  const all = strokes.flat();
  if (all.length === 0) return null;
  const xs = all.map((point) => point.x);
  const ys = all.map((point) => point.y);
  const half = penWidth / 2;
  const left = Math.min(...xs) - half;
  const top = Math.min(...ys) - half;
  return { x: left, y: top, width: Math.max(...xs) + half - left, height: Math.max(...ys) + half - top };
}

// ---- typed ----

/** handwriting-like fonts already on the computer, best first. nothing is downloaded. */
export const TYPED_STYLES = [
  { id: "script", label: "Script", family: '"Segoe Script", "Brush Script MT", "Snell Roundhand", "Apple Chancery", cursive' },
  { id: "hand", label: "Handwriting", family: '"Ink Free", "Bradley Hand", "Segoe Print", "Noteworthy", cursive' },
  { id: "formal", label: "Formal", family: '"Lucida Handwriting", "Lucida Calligraphy", "Zapfino", "Savoye LET", cursive' },
  { id: "plain", label: "Plain", family: '"Segoe UI", "Helvetica Neue", Arial, sans-serif' },
] as const;

export type TypedStyleId = (typeof TYPED_STYLES)[number]["id"];

/** the picture is drawn this big and shrunk when placed, so it stays sharp when printed. */
export const TYPED_FONT_PX = 96;

/** a signature typed with stray spaces is tidied; very long ones are cut. */
export function cleanSignatureText(text: string): string {
  return text.replace(/\s+/g, " ").trim().slice(0, 60);
}

/**
 * the canvas a typed signature is drawn on, from the width the browser
 * measured for the text: room for the letters' loops above and below,
 * and a little space at the sides.
 */
export function typedCanvasSize(measuredWidth: number, fontPx = TYPED_FONT_PX): { width: number; height: number; x: number; baseline: number } {
  const padding = Math.round(fontPx * 0.25);
  return {
    width: Math.ceil(measuredWidth) + 2 * padding,
    height: Math.round(fontPx * 1.5),
    x: padding,
    baseline: Math.round(fontPx * 1.08),
  };
}

// ---- chosen pictures ----

/**
 * makes paper white see-through, in place, on rgba pixels. pixels near
 * white go fully clear, darker ones keep their ink, and those between
 * fade, so the edges of the strokes stay smooth.
 */
export function whiteToTransparent(pixels: Uint8ClampedArray, clearAbove = 235, keepBelow = 180): void {
  for (let index = 0; index < pixels.length; index += 4) {
    const lightness = Math.min(pixels[index], pixels[index + 1], pixels[index + 2]);
    if (lightness >= clearAbove) pixels[index + 3] = 0;
    else if (lightness > keepBelow) {
      const share = (clearAbove - lightness) / (clearAbove - keepBelow);
      pixels[index + 3] = Math.round(pixels[index + 3] * share);
    }
  }
}

/** the largest size of a picture that fits in a box, keeping its proportions. */
export function fitWithin(width: number, height: number, maxWidth: number, maxHeight: number): { width: number; height: number } {
  const scale = Math.min(1, maxWidth / width, maxHeight / height);
  return { width: Math.max(1, Math.round(width * scale)), height: Math.max(1, Math.round(height * scale)) };
}

// ---- placing ----

/** a signature placed with a click is this wide on the page, in points (1.75 inches). */
export const PLACED_SIGNATURE_WIDTH = 126;

/** the picture centred in a signature field, as big as fits, in proportion. */
export function fitInField(aspect: number, field: Rect): Rect {
  let width = field.width;
  let height = width / aspect;
  if (height > field.height) {
    height = field.height;
    width = height * aspect;
  }
  return { x: field.x + (field.width - width) / 2, y: field.y + (field.height - height) / 2, width, height };
}

/** the picture centred on a point at its usual width, kept on the page. */
export function placedAround(centre: Point, aspect: number, page: PageSize): Rect {
  const width = Math.min(PLACED_SIGNATURE_WIDTH, page.width);
  const height = width / aspect;
  return keepRectOnPage({ x: centre.x - width / 2, y: centre.y - height / 2, width, height }, page);
}

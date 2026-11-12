// where free objects (free pictures and text blocks) sit on the editing
// surface, worked out from the model exactly as word places them.
//
// the model positions an object relative to the page, the margins, the
// column or its paragraph, in twips. the surface is one tall column of
// pages, so a position on page n becomes a place in the editor's root
// element: "root coordinates", css pixels at 100% zoom from the editor
// root's top-left corner (the root starts at the first page's top and
// left margins). the conversions here go both ways, so an object never
// jumps when it is floated, moved or dropped.

import type { Node as PmNode } from "prosemirror-model";

export const TWIPS_PER_PX = 15;

export interface FloatPosition {
  relativeTo: string;
  offset?: number;
  align?: string;
  offsetEmu?: number;
}

export type WrapMode = "square" | "tight" | "through" | "topAndBottom" | "behindText" | "inFrontOfText";

/** one free object as the surface needs it. */
export interface FreeObject {
  kind: "image" | "textBox";
  /** the node's position in the editor document */
  pos: number;
  /** the top-level block it belongs to (a picture's paragraph, or the text block itself) */
  block: number;
  horizontal: FloatPosition;
  vertical: FloatPosition;
  /** twips */
  width: number;
  height: number;
  wrap: WrapMode;
  zOrder: number;
  rotation: number;
}

/** the page as the surface lays it out: twips from the model, edges measured on screen. */
export interface SurfaceLayout {
  pageWidth: number;
  pageHeight: number;
  margins: { top: number; right: number; bottom: number; left: number };
  /** root coordinates of each page's top edge (page 1 is at minus the top margin) */
  pageEdges: number[];
  /** for each top-level block, the page (0-based) it starts on */
  blockPages: number[];
  /** for each top-level block, its top in root coordinates (NaN for blocks out of the flow) */
  blockTops: number[];
}

export interface Rect {
  left: number;
  top: number;
  width: number;
  height: number;
}

const px = (twips: number) => twips / TWIPS_PER_PX;

/** the free objects of a document: floating pictures in top-level paragraphs and every text block. */
export function freeObjects(doc: PmNode): FreeObject[] {
  const objects: FreeObject[] = [];
  doc.forEach((block, offset, index) => {
    const data = block.attrs.data as Record<string, unknown>;
    if (block.type.name === "text_box") {
      objects.push({
        kind: "textBox",
        pos: offset,
        block: index,
        horizontal: (data.horizontal as FloatPosition) ?? { relativeTo: "page", offset: 0 },
        vertical: (data.vertical as FloatPosition) ?? { relativeTo: "page", offset: 0 },
        width: Number(data.width ?? 2880),
        height: Number(data.height ?? 1440),
        wrap: (data.wrap as WrapMode) ?? "square",
        zOrder: Number(data.zOrder ?? 0),
        rotation: Number(data.rotation ?? 0),
      });
      return;
    }
    if (!block.isTextblock) return;
    block.forEach((child, childOffset) => {
      if (child.type.name !== "image") return;
      const image = child.attrs.data as Record<string, unknown>;
      const placement = image.placement as { type?: string; horizontal?: FloatPosition; vertical?: FloatPosition; wrap?: WrapMode } | undefined;
      if (placement?.type !== "floating") return;
      objects.push({
        kind: "image",
        pos: offset + 1 + childOffset,
        block: index,
        horizontal: placement.horizontal ?? { relativeTo: "column", offset: 0 },
        vertical: placement.vertical ?? { relativeTo: "paragraph", offset: 0 },
        width: Number(image.width ?? 1440),
        height: Number(image.height ?? 1440),
        wrap: placement.wrap ?? "square",
        zOrder: Number(image.zOrder ?? 0),
        rotation: Number(image.rotation ?? 0),
      });
    });
  });
  return objects;
}

/** the page an object belongs to: its anchor block's page (0-based). */
export function pageOf(object: FreeObject, layout: SurfaceLayout): number {
  const anchor = object.kind === "textBox" ? anchorBlock(object.block, layout.blockTops) : object.block;
  return layout.blockPages[anchor] ?? layout.blockPages[object.block] ?? 0;
}

/** a text block is anchored at the next block in the flow (word and odt do the same). */
export function anchorBlock(block: number, tops: number[]): number {
  for (let index = block + 1; index < tops.length; index += 1) if (!Number.isNaN(tops[index])) return index;
  for (let index = block - 1; index >= 0; index -= 1) if (!Number.isNaN(tops[index])) return index;
  return block;
}

/** the top of the paragraph an object is positioned from, in root coordinates. */
function anchorTop(object: FreeObject, layout: SurfaceLayout): number {
  const anchor = object.kind === "textBox" ? anchorBlock(object.block, layout.blockTops) : object.block;
  const top = layout.blockTops[anchor];
  return Number.isNaN(top) || top === undefined ? (layout.pageEdges[pageOf(object, layout)] ?? 0) + px(layout.margins.top) : top;
}

/** x in twips from the page's left edge. */
export function pageX(position: FloatPosition, width: number, layout: SurfaceLayout): number {
  const { margins, pageWidth } = layout;
  const textWidth = pageWidth - margins.left - margins.right;
  const offset = position.offset ?? 0;
  const relativeToPage = position.relativeTo === "page";
  const start = relativeToPage ? 0 : margins.left;
  const room = relativeToPage ? pageWidth : textWidth;
  switch (position.align) {
    case "center":
      return start + (room - width) / 2;
    case "right":
    case "outside":
      return start + room - width;
    case "left":
    case "inside":
      return start;
    default:
      return start + offset;
  }
}

/** an object's box in root coordinates. */
export function objectRect(object: FreeObject, layout: SurfaceLayout): Rect {
  const page = pageOf(object, layout);
  const edge = layout.pageEdges[page] ?? layout.pageEdges[0] ?? -px(layout.margins.top);
  const left = px(pageX(object.horizontal, object.width, layout) - layout.margins.left);
  const offset = object.vertical.offset ?? 0;
  let top: number;
  switch (object.vertical.relativeTo) {
    case "page":
      top = edge + px(verticalAligned(object.vertical, offset, object.height, 0, layout.pageHeight));
      break;
    case "margin":
    case "topMargin":
      top = edge + px(layout.margins.top + verticalAligned(object.vertical, offset, object.height, 0, layout.pageHeight - layout.margins.top - layout.margins.bottom));
      break;
    default:
      // paragraph and line: from the anchor paragraph's top
      top = anchorTop(object, layout) + px(offset);
  }
  return { left, top, width: px(object.width), height: px(object.height) };
}

function verticalAligned(position: FloatPosition, offset: number, height: number, start: number, room: number): number {
  if (position.align === "center") return start + (room - height) / 2;
  if (position.align === "bottom" || position.align === "outside") return start + room - height;
  if (position.align === "top" || position.align === "inside") return start;
  return start + offset;
}

/** the page an object at root y `top` lands on (0-based): the last page whose edge is above it. */
export function pageAt(top: number, layout: SurfaceLayout): number {
  let page = 0;
  layout.pageEdges.forEach((edge, index) => {
    if (edge <= top + 0.5) page = index;
  });
  return page;
}

/**
 * root coordinates back to page coordinates (twips from the page corner)
 * on `page`: what the engine's float_image hint and move_to take.
 */
export function toPageTwips(left: number, top: number, page: number, layout: SurfaceLayout): { x: number; y: number } {
  const edge = layout.pageEdges[page] ?? 0;
  return { x: Math.round((left + px(layout.margins.left)) * TWIPS_PER_PX), y: Math.round((top - edge) * TWIPS_PER_PX) };
}

/** a page-relative position for the model, as move_to writes it. */
export function pagePosition(x: number, y: number): { horizontal: FloatPosition; vertical: FloatPosition } {
  return { horizontal: { relativeTo: "page", offset: Math.round(x) }, vertical: { relativeTo: "page", offset: Math.round(y) } };
}

/** z-index on screen: behind the text below it, everything else above, in stacking order. */
export function zIndexFor(object: Pick<FreeObject, "wrap" | "zOrder">): number {
  // z-index takes whole numbers; stacking numbers above 9999 are rare
  const order = Math.max(0, Math.min(9999, Math.round(object.zOrder || 0)));
  return object.wrap === "behindText" ? order - 10000 : 3 + order;
}

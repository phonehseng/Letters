// reading the editing surface back after it is drawn: where each block
// and each page edge ended up, in the editor root's coordinates (css
// pixels at 100% zoom). from that, the object layer is worked out: where
// each free object goes and which paragraphs carry the floats that make
// text wrap around them. the maths is in objectGeometry.ts and wrap.ts;
// this file only measures and puts the pieces together.

import type { Node as PmNode } from "prosemirror-model";
import type { EditorView } from "prosemirror-view";
import { rotatedBounds } from "./crop";
import { freeObjects, objectRect, zIndexFor, type SurfaceLayout } from "./objectGeometry";
import type { Placement } from "./objectLayer";
import type { PageGap } from "./plugins";
import { exclusionFor, type Exclusion } from "./wrap";

export interface MeasuredBlocks {
  /** each top-level block's top, NaN for text blocks (they are out of the flow) */
  tops: number[];
  bottoms: number[];
  holdsText: boolean[];
}

/** where each top-level block is now. */
export function measureBlocks(view: EditorView, zoom: number): MeasuredBlocks | null {
  const root = view.dom.getBoundingClientRect();
  const tops: number[] = [];
  const bottoms: number[] = [];
  const holdsText: boolean[] = [];
  let pos = 0;
  const doc = view.state.doc;
  for (let index = 0; index < doc.childCount; index += 1) {
    const node = doc.child(index);
    const element = view.nodeDOM(pos) as HTMLElement | null;
    if (!element?.getBoundingClientRect) return null;
    const rect = element.getBoundingClientRect();
    const inFlow = node.type.name !== "text_box";
    tops.push(inFlow ? (rect.top - root.top) / zoom : Number.NaN);
    bottoms.push(inFlow ? (rect.bottom - root.top) / zoom : Number.NaN);
    holdsText.push(node.isTextblock);
    pos += node.nodeSize;
  }
  return { tops, bottoms, holdsText };
}

/** each page's top edge: page 1 sits a top margin above the root; the others follow their gap. */
export function measurePageEdges(view: EditorView, zoom: number, gaps: PageGap[], topMargin: number): number[] {
  const root = view.dom.getBoundingClientRect();
  const edges = [-topMargin];
  const elements = [...view.dom.querySelectorAll<HTMLElement>(":scope .w-page-gap")];
  elements.forEach((element, index) => {
    const rect = element.getBoundingClientRect();
    const height = gaps[index]?.height ?? rect.height / zoom;
    edges.push((rect.top - root.top) / zoom + height - topMargin);
  });
  return edges;
}

/** the page each block starts on (0-based), from the gaps' block indexes. */
export function blockPagesFrom(gaps: PageGap[], blockCount: number): number[] {
  const starts = gaps.filter((gap) => gap.pos === undefined).map((gap) => gap.blockIndex);
  const pages: number[] = [];
  let page = 0;
  let inlineBefore = 0;
  for (let index = 0; index < blockCount; index += 1) {
    while (page < starts.length && starts[page] <= index) page += 1;
    // a page that starts inside a paragraph counts for the blocks after it
    inlineBefore = gaps.filter((gap) => gap.pos !== undefined && gap.blockIndex < index).length;
    pages.push(page + inlineBefore);
  }
  return pages;
}

/** the text nodes of an element, leaving out widgets (page gaps, wrap floats, word's breaks). */
function textNodes(element: HTMLElement): Text[] {
  const walker = document.createTreeWalker(element, NodeFilter.SHOW_TEXT, {
    acceptNode: (node) => (node.parentElement?.closest(".w-page-gap, .w-exclusion") ? NodeFilter.FILTER_REJECT : NodeFilter.FILTER_ACCEPT),
  });
  const found: Text[] = [];
  for (let node = walker.nextNode(); node; node = walker.nextNode()) found.push(node as Text);
  return found;
}

/**
 * where each line of a paragraph ends, from the paragraph's top, with any
 * page gap drawn inside it taken out (css pixels at 100%). lines are the
 * text's own boxes, grouped by their top.
 */
export function measureLineBottoms(element: HTMLElement, zoom: number): number[] {
  const top = element.getBoundingClientRect().top;
  const gaps = [...element.querySelectorAll<HTMLElement>(":scope .w-page-gap")].map((gap) => gap.getBoundingClientRect());
  const shiftFor = (y: number) => gaps.filter((gap) => gap.top <= y).reduce((sum, gap) => sum + gap.height, 0);
  const lines: { top: number; bottom: number }[] = [];
  const range = document.createRange();
  for (const node of textNodes(element)) {
    range.selectNodeContents(node);
    for (const rect of range.getClientRects()) {
      if (rect.width === 0 && rect.height === 0) continue;
      const shift = shiftFor(rect.top);
      const lineTop = (rect.top - top - shift) / zoom;
      const lineBottom = (rect.bottom - top - shift) / zoom;
      const same = lines.find((line) => Math.abs(line.top - lineTop) < 2 || (lineTop < line.bottom - 2 && lineBottom > line.top + 2));
      if (same) {
        same.top = Math.min(same.top, lineTop);
        same.bottom = Math.max(same.bottom, lineBottom);
      } else lines.push({ top: lineTop, bottom: lineBottom });
    }
  }
  return lines.sort((a, b) => a.top - b.top).map((line) => line.bottom);
}

/**
 * the document position where line `line` of a paragraph starts, found
 * from the screen: the first text on that line.
 */
export function lineStartPos(view: EditorView, element: HTMLElement, line: number, zoom: number): number | null {
  const bottoms = measureLineBottoms(element, zoom);
  if (line <= 0 || line >= bottoms.length) return null;
  const top = element.getBoundingClientRect().top;
  const wanted = bottoms[line - 1];
  const range = document.createRange();
  for (const node of textNodes(element)) {
    range.selectNodeContents(node);
    for (const rect of range.getClientRects()) {
      if ((rect.top - top) / zoom >= wanted - 1) {
        // the first character at or after the line's top
        const found = view.posAtCoords({ left: rect.left + 0.5, top: rect.top + rect.height / 2 });
        return found ? found.pos : null;
      }
    }
  }
  return null;
}

/** the heights of text blocks that grow with their text, as drawn. */
export function measuredBoxHeights(view: EditorView, doc: PmNode, zoom: number): Map<number, number> {
  const heights = new Map<number, number>();
  doc.forEach((node, offset) => {
    if (node.type.name !== "text_box" || !(node.attrs.data as { autoHeight?: boolean }).autoHeight) return;
    const element = view.nodeDOM(offset) as HTMLElement | null;
    if (element) heights.set(offset, (element.getBoundingClientRect().height / zoom) * 15);
  });
  return heights;
}

export interface Layer {
  placements: Placement[];
  exclusions: Exclusion[];
}

/** where every free object goes and the floats text wraps around. */
export function computeLayer(doc: PmNode, layout: SurfaceLayout, columnWidth: number, boxHeights: Map<number, number>): Layer {
  const placements: Placement[] = [];
  const exclusions: Exclusion[] = [];
  const pageTextBottom = (top: number) => {
    let edge = layout.pageEdges[0] ?? 0;
    for (const each of layout.pageEdges) if (each <= top + 0.5) edge = each;
    return edge + (layout.pageHeight - layout.margins.bottom) / 15;
  };
  for (const object of freeObjects(doc)) {
    const sized = { ...object, height: boxHeights.get(object.pos) ?? object.height };
    const rect = objectRect(sized, layout);
    placements.push({ pos: object.pos, left: rect.left, top: rect.top, zIndex: zIndexFor(object), rotation: object.rotation });
    const bounds = object.rotation ? rotatedBounds(rect, object.rotation) : rect;
    const exclusion = exclusionFor(bounds, object.wrap, { columnWidth, blockTops: layout.blockTops, holdsText: hostable(doc, layout), pageTextBottom });
    if (exclusion) exclusions.push(exclusion);
  }
  return { placements, exclusions };
}

function hostable(doc: PmNode, layout: SurfaceLayout): boolean[] {
  return layout.blockTops.map((_, index) => !!doc.maybeChild(index)?.isTextblock);
}

/** two placement lists are the same on screen, to the hundredth of a pixel. */
export function samePlacements(a: Placement[], b: Placement[]): boolean {
  return (
    a.length === b.length &&
    a.every((one, index) => {
      const other = b[index];
      return one.pos === other.pos && Math.abs(one.left - other.left) < 0.01 && Math.abs(one.top - other.top) < 0.01 && one.zIndex === other.zIndex && one.rotation === other.rotation;
    })
  );
}

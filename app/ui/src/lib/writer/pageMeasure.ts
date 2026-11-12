// measuring the editing surface for pagination (PageSurface.svelte): each
// top-level block's height, lines, keep rules and footnote references, and
// where a page that starts inside a paragraph begins. everything is read
// from the screen in css pixels at 100% zoom (screen pixels / zoom).

import type { EditorView } from "prosemirror-view";
import type { DocumentMeta, ParagraphData } from "./model";
import type { BlockBox, Pages } from "./pagination";
import type { PageGap } from "./plugins";
import { pagesKey } from "./plugins";
import { resolvedParagraphProps } from "./styles";
import { lineStartPos, measureLineBottoms } from "./surfaceLayout";
import { blockPos, editorOffset } from "./textBlocks";

/** a paragraph needs four lines (about 60px) before it may be split between pages: two on each side */
export const SPLITTABLE = 60;

/** a footnote's height until it has been drawn once */
const UNDRAWN_NOTE_HEIGHT = 16;

export interface MeasureContext {
  view: EditorView;
  zoom: number;
  meta: DocumentMeta;
  /** the ids of the document's footnotes (not endnotes) */
  footnoteIds: Set<string>;
  /** each footnote's drawn height, by id */
  noteHeights: Record<string, number>;
}

type NoteRef = { line: number; height: number };

/** the top-level blocks' elements, tops and bottoms (NaN for text blocks, which are out of the flow). */
function blockElements(context: MeasureContext): { elements: HTMLElement[]; tops: number[]; bottoms: number[] } | null {
  const { view, zoom } = context;
  const doc = view.state.doc;
  const elements: HTMLElement[] = [];
  const tops: number[] = [];
  const bottoms: number[] = [];
  let pos = 0;
  for (let index = 0; index < doc.childCount; index += 1) {
    const element = view.nodeDOM(pos) as HTMLElement | null;
    if (!element?.getBoundingClientRect) return null;
    const rect = element.getBoundingClientRect();
    const inFlow = doc.child(index).type.name !== "text_box";
    const marginBottom = Number.parseFloat(getComputedStyle(element).marginBottom) || 0;
    elements.push(element);
    tops.push(inFlow ? rect.top / zoom : Number.NaN);
    bottoms.push(inFlow ? rect.bottom / zoom + marginBottom : Number.NaN);
    pos += doc.child(index).nodeSize;
  }
  return { elements, tops, bottoms };
}

/** page gap heights: before a block, and inside one (a page starting mid-paragraph). */
function gapHeights(gaps: PageGap[]): { before: Map<number, number>; inside: Map<number, number> } {
  const before = new Map<number, number>();
  const inside = new Map<number, number>();
  for (const gap of gaps) {
    const map = gap.pos === undefined ? before : inside;
    map.set(gap.blockIndex, (map.get(gap.blockIndex) ?? 0) + gap.height);
  }
  return { before, inside };
}

/** how tall each top-level block is, its lines, notes and keep rules, with the page gaps taken out. */
export function blockBoxes(context: MeasureContext): BlockBox[] | null {
  const { view, zoom, meta } = context;
  const measured = blockElements(context);
  if (!measured) return null;
  const { elements, tops, bottoms } = measured;
  const gaps = gapHeights(pagesKey.getState(view.state) ?? []);
  const nextTop = (index: number) => {
    for (let next = index + 1; next < tops.length; next += 1) {
      if (!Number.isNaN(tops[next])) return tops[next] - (gaps.before.get(next) ?? 0);
    }
    return bottoms[index];
  };
  const refs = noteRefLines(context, elements);
  return tops.map((top, index) => {
    if (Number.isNaN(top)) return { height: 0 };
    const node = view.state.doc.child(index);
    const height = Math.max(0, nextTop(index) - top - (gaps.inside.get(index) ?? 0));
    const data = node.attrs.data as ParagraphData;
    const props = resolvedParagraphProps(meta, data?.styleId, data?.props);
    const splittable = node.isTextblock && height > SPLITTABLE;
    return {
      height,
      isPageBreak: node.type.name === "page_break",
      breakBefore: !!props.pageBreakBefore,
      keepWithNext: !!props.keepWithNext,
      keepTogether: !!props.keepLinesTogether,
      lineBottoms: splittable ? measureLineBottoms(elements[index], zoom) : undefined,
      notes: refs.get(index),
    };
  });
}

/** each block's footnote references: on which line, and how tall the note is (measured where it is drawn). */
function noteRefLines(context: MeasureContext, elements: HTMLElement[]): Map<number, NoteRef[]> {
  const { zoom, footnoteIds, noteHeights } = context;
  const found = new Map<number, NoteRef[]>();
  elements.forEach((element, index) => {
    const refs = element.querySelectorAll<HTMLElement>("sup.w-note-ref");
    if (!refs.length) return;
    for (const ref of refs) {
      const id = ref.dataset.note ?? "";
      if (!footnoteIds.has(id)) continue;
      const note = { line: lineOf(element, ref, zoom), height: noteHeights[id] ?? UNDRAWN_NOTE_HEIGHT };
      found.set(index, [...(found.get(index) ?? []), note]);
    }
  });
  return found;
}

/** the line of a paragraph a reference sits on (0 for paragraphs too short to split). */
export function lineOf(element: HTMLElement, ref: HTMLElement, zoom: number): number {
  if (element.getBoundingClientRect().height <= SPLITTABLE) return 0;
  const y = (ref.getBoundingClientRect().bottom - element.getBoundingClientRect().top) / zoom;
  const line = measureLineBottoms(element, zoom).findIndex((bottom) => bottom >= y - 1);
  return Math.max(0, line);
}

type Inset = { left: number; right: number };

/** where each page that starts inside a paragraph starts: a line (estimate) or Word's character offset. */
export function breakPositions(view: EditorView, pages: Pages, zoom: number): { positions: (number | undefined)[]; insets: (Inset | undefined)[] } {
  const positions: (number | undefined)[] = [];
  const insets: (Inset | undefined)[] = [];
  const doc = view.state.doc;
  for (const start of pages.breaks ?? []) {
    const offset = (start as { offset?: number }).offset ?? 0;
    const node = doc.maybeChild(start.block);
    const at = blockPos(doc, start.block);
    const element = view.nodeDOM(at) as HTMLElement | null;
    let pos: number | undefined;
    if (node?.isTextblock && offset > 0) {
      pos = at + 1 + editorOffset(node, offset);
    } else if (node?.isTextblock && element && start.line > 0) {
      pos = lineStartPos(view, element, start.line, zoom) ?? undefined;
    }
    positions.push(pos);
    insets.push(pos !== undefined && element ? paragraphInset(view, element, zoom) : undefined);
  }
  return { positions, insets };
}

/** how far a paragraph's content sits in from the text column on each side. */
function paragraphInset(view: EditorView, element: HTMLElement, zoom: number): Inset {
  const root = view.dom.getBoundingClientRect();
  const rect = element.getBoundingClientRect();
  const style = getComputedStyle(element);
  return {
    left: (rect.left - root.left) / zoom + (Number.parseFloat(style.paddingLeft) || 0),
    right: (root.right - rect.right) / zoom + (Number.parseFloat(style.paddingRight) || 0),
  };
}

/** the page (0-based) each footnote goes on: the page its reference is on. */
export function assignNotes(context: MeasureContext, pages: Pages): Map<number, string[]> {
  const { view, zoom, footnoteIds } = context;
  const byPage = new Map<number, string[]>();
  const breaks = pages.breaks ?? pages.starts.map((block) => ({ block, line: 0 }));
  view.state.doc.forEach((node, offset, index) => {
    const element = view.nodeDOM(offset) as HTMLElement | null;
    node.descendants((child) => {
      if (child.type.name !== "note_ref") return true;
      const id = String((child.attrs.data as { noteId?: string }).noteId);
      if (!footnoteIds.has(id)) return false;
      const ref = element?.querySelector<HTMLElement>(`sup.w-note-ref[data-note="${CSS.escape(id)}"]`);
      const line = ref && element ? lineOf(element, ref, zoom) : 0;
      let page = 0;
      breaks.forEach((start, number) => {
        if (start.block < index || (start.block === index && start.line <= line)) page = number;
      });
      byPage.set(page, [...(byPage.get(page) ?? []), id]);
      return false;
    });
  });
  return byPage;
}

/** two gap lists are the same on screen (to a pixel). */
export function sameGaps(a: PageGap[], b: PageGap[]): boolean {
  return a.length === b.length && a.every((gap, index) => gap.blockIndex === b[index].blockIndex && gap.pos === b[index].pos && Math.abs(gap.height - b[index].height) < 1);
}

// the page tool: whatever a press, drag or key on a page means right now.
//
// exactly one page tool is active at a time. the default is Select
// (select text, click objects); the Annotate tools add Highlight, Note,
// Text Box, Shapes, Draw and Picture; later stages add redaction ("draw
// boxes or select text to redact") and forms ("click a field to fill").
//
// the viewer turns pointer events on the pages into PagePointers (page
// points, at any zoom, in either page mode) and hands them to the active
// tool. the tool keeps its own state, and says what to draw on each page
// while a gesture is under way (`marks`); the real result is drawn by the
// engine after the tool's edit, through the normal tile refresh.
//
// a new tool is usually one call to a factory in tools.svelte.ts (a
// rectangle, a line, a point, freehand ink, or text selection) with a
// callback that makes the edit. see "Adding a page tool" in ARCHITECTURE.md.

import type { PagePoint, Point, Rect } from "./pagePoints";

/** a pointer on a page: where (in points), with which keys, how many clicks. */
export interface PagePointer extends PagePoint {
  /** css pixels per point on this page, to tell a click from a small drag */
  scale: number;
  shiftKey: boolean;
  altKey: boolean;
  /** 1, 2 for a double click, 3 for a triple click */
  clicks: number;
  /** milliseconds, from performance.now() */
  time: number;
}

/** how a live preview looks. colours are css colours; widths are in points. */
export interface MarkStyle {
  stroke: string | null;
  fill: string | null;
  width: number;
  opacity: number;
  /** dash and gap lengths in points */
  dash?: number[];
}

/** something a tool draws over a page while a gesture is under way. */
export type Mark =
  | { kind: "rect"; rect: Rect; style: MarkStyle }
  | { kind: "ellipse"; rect: Rect; style: MarkStyle }
  | { kind: "line"; start: Point; end: Point; arrow: boolean; style: MarkStyle }
  | { kind: "ink"; strokes: Point[][]; style: MarkStyle }
  /** the selection colour over text, as boxes */
  | { kind: "textBoxes"; rects: Rect[] };

/** the rest of a press after it started: a drag and its end. */
export interface Gesture {
  move(pointer: PagePointer): void;
  up(pointer: PagePointer): void;
  cancel(): void;
}

export interface PageTool {
  /** unique, such as "select", "highlight", "rectangle", "redact-area" */
  readonly id: string;
  /** the css cursor over pages while this tool is active */
  readonly cursor: string;
  /**
   * a drag follows the pointer onto other pages (text selection does),
   * or stays on the page it started on (drawing does)
   */
  readonly crossesPages: boolean;
  /** a press on a page. returns the gesture to follow, or null to ignore the press */
  down(pointer: PagePointer): Gesture | null;
  /** live previews to draw on one page */
  marks(page: number): Mark[];
  /** a key while the document has the focus. true when the tool used it */
  key?(event: KeyboardEvent): boolean;
  /** another tool takes over: save or drop anything still waiting */
  finish?(): void;
}

/** a tool that does nothing, for while the document is changing. */
export const IDLE_TOOL: PageTool = {
  id: "idle",
  cursor: "default",
  crossesPages: false,
  down: () => null,
  marks: () => [],
};

/** turns an "rgb" colour from the engine into css, with an opacity. */
export function cssColor(color: { r: number; g: number; b: number }, opacity = 1): string {
  return opacity >= 1 ? `rgb(${color.r} ${color.g} ${color.b})` : `rgb(${color.r} ${color.g} ${color.b} / ${opacity})`;
}

// ready-made page tools: drag a rectangle, drag a line, place a point,
// draw freehand, and select text. each takes a callback that makes the
// edit, so a new tool (an annotation shape, a redaction box, a form
// field) is one call with a few settings. the arithmetic is in
// gestures.ts and textSelection.ts, where it is unit tested.

import type { PageSize } from "../layout";
import {
  addStroke,
  addStrokePoint,
  drawingIsDue,
  fitAspect,
  lineFromDrag,
  movedEnough,
  rectAround,
  rectFromDrag,
  STROKE_GROUP_PAUSE_MS,
  type PendingDrawing,
} from "./gestures";
import type { Point, Rect } from "./pagePoints";
import type { Gesture, MarkStyle, PagePointer, PageTool } from "./pageTool";
import {
  caretAt,
  caretMoveForKey,
  extendByKey,
  extendSelection,
  granularityFor,
  isCollapsed,
  selectionAtPageStart,
  startSelection,
  type PageText,
  type TextSelection,
} from "./textSelection";

// ---- a rectangle ----

export interface RectangleToolOptions {
  id: string;
  cursor?: string;
  /** how the preview is drawn: a box or an ellipse inside the box */
  shape?: "rect" | "ellipse";
  style: () => MarkStyle;
  /** the page's size in points, to keep the rectangle on it */
  pageSize: (page: number) => PageSize | undefined;
  /** shift makes a square (or circle) */
  squareWithShift?: boolean;
  /** keeps these proportions (width over height), as a picture does */
  aspect?: () => number | null;
  /** a click without a drag makes a rectangle this big around the point; leave out to ignore clicks */
  clickSize?: () => { width: number; height: number } | null;
  /** drags smaller than this, in points, are ignored */
  minSize?: number;
  onDraw: (page: number, rect: Rect) => void;
}

/** drag out a rectangle. what redaction's "draw boxes" and the Shapes tool use. */
export function rectangleTool(options: RectangleToolOptions): PageTool & { readonly preview: { page: number; rect: Rect } | null } {
  let preview = $state<{ page: number; rect: Rect } | null>(null);
  const minSize = options.minSize ?? 2;

  function rectFor(start: PagePointer, now: PagePointer, size: PageSize): Rect {
    const square = (options.squareWithShift ?? false) && now.shiftKey;
    const rect = rectFromDrag(start, now, size, square);
    const aspect = options.aspect?.() ?? null;
    return aspect ? fitAspect(rect, aspect, start) : rect;
  }

  return {
    id: options.id,
    cursor: options.cursor ?? "crosshair",
    crossesPages: false,
    get preview() {
      return preview;
    },
    down(start) {
      const size = options.pageSize(start.page);
      if (!size) return null;
      preview = { page: start.page, rect: { x: start.x, y: start.y, width: 0, height: 0 } };
      return {
        move(now) {
          preview = { page: start.page, rect: rectFor(start, now, size) };
        },
        up(now) {
          preview = null;
          const rect = rectFor(start, now, size);
          if (movedEnough(start, now, start.scale) && rect.width >= minSize && rect.height >= minSize) {
            options.onDraw(start.page, rect);
            return;
          }
          const clickSize = options.clickSize?.();
          if (clickSize) options.onDraw(start.page, rectAround(start, clickSize.width, clickSize.height, size));
        },
        cancel() {
          preview = null;
        },
      };
    },
    marks(page) {
      if (!preview || preview.page !== page) return [];
      const kind = options.shape === "ellipse" ? "ellipse" : "rect";
      return [{ kind, rect: preview.rect, style: options.style() }];
    },
  };
}

// ---- a line ----

export interface LineToolOptions {
  id: string;
  style: () => MarkStyle;
  arrow: () => boolean;
  pageSize: (page: number) => PageSize | undefined;
  onDraw: (page: number, start: Point, end: Point) => void;
}

/** drag a straight line; shift keeps it to 45 degree steps. */
export function lineTool(options: LineToolOptions): PageTool {
  let preview = $state<{ page: number; start: Point; end: Point } | null>(null);
  return {
    id: options.id,
    cursor: "crosshair",
    crossesPages: false,
    down(start) {
      const size = options.pageSize(start.page);
      if (!size) return null;
      const from = { x: start.x, y: start.y };
      return {
        move(now) {
          preview = { page: start.page, ...lineFromDrag(from, now, size, now.shiftKey) };
        },
        up(now) {
          preview = null;
          if (!movedEnough(start, now, start.scale)) return;
          const line = lineFromDrag(from, now, size, now.shiftKey);
          options.onDraw(start.page, line.start, line.end);
        },
        cancel() {
          preview = null;
        },
      };
    },
    marks(page) {
      if (!preview || preview.page !== page) return [];
      return [{ kind: "line", start: preview.start, end: preview.end, arrow: options.arrow(), style: options.style() }];
    },
  };
}

// ---- a point ----

export interface PointToolOptions {
  id: string;
  cursor?: string;
  onPlace: (page: number, point: Point) => void;
}

/** click to place something: a note, later a form field or a signature. */
export function pointTool(options: PointToolOptions): PageTool {
  return {
    id: options.id,
    cursor: options.cursor ?? "copy",
    crossesPages: false,
    down(start) {
      return {
        move() {},
        up(now) {
          // a press that slid a little still places at the start
          if (!movedEnough(start, now, start.scale / 4)) options.onPlace(start.page, { x: start.x, y: start.y });
        },
        cancel() {},
      };
    },
    marks: () => [],
  };
}

// ---- freehand ink ----

export interface InkStyle {
  style: MarkStyle;
  /** strokes in different styles never join one drawing */
  key: string;
}

export interface InkToolOptions {
  id: string;
  style: () => InkStyle;
  /** a finished drawing: every stroke of one gesture */
  onDrawing: (page: number, strokes: Point[][]) => void;
  /** for tests: the clock and the timer */
  now?: () => number;
  later?: (run: () => void, ms: number) => unknown;
}

/**
 * draw with the pointer. strokes made within a short pause of each other
 * on one page become one drawing, saved when the pause is over (or when
 * the tool is put away), so undo takes back a whole word or sketch.
 */
export function inkTool(options: InkToolOptions): PageTool & { readonly pending: PendingDrawing | null } {
  const now = options.now ?? (() => performance.now());
  const later = options.later ?? ((run: () => void, ms: number) => setTimeout(run, ms));
  let pending = $state<PendingDrawing | null>(null);
  let current = $state<{ page: number; points: Point[] } | null>(null);

  function save(drawing: PendingDrawing | null) {
    if (drawing && drawing.strokes.length > 0) options.onDrawing(drawing.page, drawing.strokes);
  }

  function saveWhenDue() {
    if (drawingIsDue(pending, now())) {
      const drawing = pending;
      pending = null;
      save(drawing);
    }
  }

  function endStroke() {
    if (!current) return;
    const stroke = current;
    current = null;
    const added = addStroke(pending, stroke.page, stroke.points, now(), options.style().key);
    pending = added.pending;
    save(added.finished);
    later(saveWhenDue, STROKE_GROUP_PAUSE_MS + 50);
  }

  return {
    id: options.id,
    cursor: "crosshair",
    crossesPages: false,
    get pending() {
      return pending;
    },
    down(start) {
      current = { page: start.page, points: [{ x: start.x, y: start.y }] };
      return {
        move(point) {
          if (current) current = { page: current.page, points: addStrokePoint(current.points, { x: point.x, y: point.y }) };
        },
        up(point) {
          if (current) current = { page: current.page, points: addStrokePoint(current.points, { x: point.x, y: point.y }) };
          endStroke();
        },
        cancel() {
          current = null;
        },
      };
    },
    marks(page) {
      const strokes: Point[][] = [];
      if (pending?.page === page) strokes.push(...pending.strokes);
      if (current?.page === page) strokes.push(current.points);
      return strokes.length > 0 ? [{ kind: "ink", strokes, style: options.style().style }] : [];
    },
    finish() {
      const drawing = pending;
      pending = null;
      current = null;
      save(drawing);
    },
  };
}

// ---- text selection ----

/** what a text tool needs from the viewer: page text and the one shared selection. */
export interface TextHost {
  text: (page: number) => PageText | undefined;
  selection: () => TextSelection | null;
  setSelection: (selection: TextSelection | null) => void;
  /** the page being read, where a selection made from the keyboard starts */
  currentPage: () => number;
  pageCount: () => number;
}

export interface TextToolOptions {
  id: string;
  cursor?: string;
  host: TextHost;
  /**
   * asked first about every press. the Select tool uses it for objects
   * on the page (annotations now, form fields later): a gesture back
   * means the press was about an object, not text.
   */
  pick?: (pointer: PagePointer) => Gesture | null;
  /** a drag across text has finished. Highlight and the other marks apply it here */
  onSelected?: (selection: TextSelection) => void;
}

/**
 * select text by dragging, double-clicking (a word) or triple-clicking
 * (a line), across lines and pages; shift-click and shift with the arrow
 * keys extend it. the Select tool and the text mark tools are this tool.
 */
export function textTool(options: TextToolOptions): PageTool {
  const { host } = options;
  return {
    id: options.id,
    cursor: options.cursor ?? "text",
    crossesPages: true,
    down(pointer) {
      const picked = options.pick?.(pointer);
      if (picked) return picked;
      const text = host.text(pointer.page);
      if (!text || text.chars.length === 0) {
        host.setSelection(null);
        return null;
      }
      const caret = caretAt(text, pointer.page, pointer);
      const existing = host.selection();
      let selection =
        pointer.shiftKey && existing
          ? extendSelection({ ...existing, granularity: "character", origin: { start: existing.anchor, end: existing.anchor } }, text, caret)
          : startSelection(text, caret, granularityFor(pointer.clicks));
      host.setSelection(selection);
      return {
        move(now) {
          const pageText = host.text(now.page);
          if (!pageText || pageText.chars.length === 0) return;
          selection = extendSelection(selection, pageText, caretAt(pageText, now.page, now));
          host.setSelection(selection);
        },
        up() {
          if (isCollapsed(selection)) {
            host.setSelection(null);
            return;
          }
          options.onSelected?.(selection);
        },
        cancel() {
          host.setSelection(null);
        },
      };
    },
    marks: () => [],
    key(event) {
      if (!event.shiftKey) return false;
      const move = caretMoveForKey(event.key, event.ctrlKey || event.altKey);
      if (!move) return false;
      const start = host.selection() ?? selectionAtPageStart(host.currentPage());
      host.setSelection(extendByKey(start, move, host.text, host.pageCount()));
      return true;
    },
  };
}

// everything the Annotate tools share while a document is open: which
// page tool is active, the text selection, the annotations on the pages,
// the one that is selected, a note or text box being written, and the
// style each tool will use next.
//
// the viewer makes one of these and hands it to the pages (for drawing
// and pointer input) and to the Annotate tab (for its controls). every
// change to the document goes through the viewer's change queue, so it
// is one undo step and marks the document edited; the result is drawn by
// the engine and arrives with the normal tile refresh.

import type { DocumentState } from "../api";
import type { PageSize } from "../layout";
import { moveFrame, sameRect } from "../pageTools/objectFrame";
import { rectAround } from "../pageTools/gestures";
import type { PagePoint, Point, Rect } from "../pageTools/pagePoints";
import { PageTextStore } from "../pageTools/pageTextStore.svelte";
import { cssColor, type Gesture, type MarkStyle, type PagePointer, type PageTool } from "../pageTools/pageTool";
import { selectedPages, selectedText, selectionRects, type TextSelection } from "../pageTools/textSelection";
import { inkTool, lineTool, pointTool, rectangleTool, textTool, type TextHost } from "../pageTools/tools.svelte";
import { addedAnnouncement, annotationAt, frameBehaviour, kindLabel } from "./annotations";
import {
  addImage,
  addInk,
  addLine,
  addNote,
  addReply,
  addShape,
  addTextBox,
  addTextMarkups,
  deleteAnnotation,
  listAllAnnotations,
  pageText,
  updateAnnotation,
  type AnnotationChanges,
  type AnnotationEdit,
  type AnnotationInfo,
  type Rgb,
  type ShapeSettings,
  type TextAlign,
  type TextMarkupKind,
} from "./annotateApi";
import { fontParts, standardFont, type FontFamily } from "./annotateEffects";
import { HIGHLIGHT_SWATCHES, HIGHLIGHTER, INK_SWATCHES, NOTE_SWATCHES } from "./palette";

/** the page tools of the Annotate tab, plus Select. */
export type AnnotateToolId = "select" | "highlight" | "underline" | "strikeOut" | "note" | "textBox" | "shape" | "draw" | "picture";

export type ShapeChoice = "square" | "circle" | "line" | "arrow";

/** what the viewer lends the session. */
export interface AnnotateHost {
  documentId: () => number;
  pageSize: (page: number) => PageSize | undefined;
  pageCount: () => number;
  currentPage: () => number;
  /** the middle of what is on screen, on the page being read: where keyboard placement puts things */
  viewCentre: () => PagePoint;
  /** runs a change through the window's queue; false when it failed (the error is already shown) */
  change: (action: () => Promise<DocumentState>) => Promise<boolean>;
  /** says something to screen readers */
  announce: (text: string) => void;
  /** scrolls so a place on a page is in view */
  reveal: (page: number, rect: Rect) => void;
}

/** the style each tool uses next. */
export interface AnnotateStyles {
  marks: Record<TextMarkupKind, Rgb>;
  note: Rgb;
  textBox: { family: FontFamily; bold: boolean; italic: boolean; size: number; color: Rgb; align: TextAlign; fill: Rgb | null };
  shape: { kind: ShapeChoice; color: Rgb; width: number; fill: Rgb | null; opacity: number };
  ink: { color: Rgb; width: number; opacity: number; highlighter: boolean };
}

export function defaultStyles(): AnnotateStyles {
  return {
    marks: {
      highlight: HIGHLIGHT_SWATCHES[0].color,
      underline: INK_SWATCHES[2].color,
      strikeOut: INK_SWATCHES[1].color,
      squiggly: INK_SWATCHES[1].color,
    },
    note: NOTE_SWATCHES[0].color,
    textBox: { family: "helvetica", bold: false, italic: false, size: 12, color: INK_SWATCHES[0].color, align: "left", fill: null },
    shape: { kind: "square", color: INK_SWATCHES[1].color, width: 2, fill: null, opacity: 1 },
    ink: { color: INK_SWATCHES[2].color, width: 2, opacity: 1, highlighter: false },
  };
}

/** a note being written: a new one at a place, or one that exists. */
export interface NoteDraft {
  page: number;
  /** the icon's top left corner */
  position: Point;
  /** null while the note is new */
  id: string | null;
  text: string;
}

/** a text box being typed, on the page, before it becomes an annotation. */
export interface TextBoxDraft {
  page: number;
  rect: Rect;
  /** null while the box is new */
  id: string | null;
  text: string;
}

/** the size of a note's icon, as the engine draws it. */
export const NOTE_ICON_SIZE = 20;
/** a text box placed with a click, or from the keyboard. */
export const NEW_TEXT_BOX = { width: 180, height: 36 };
/** a shape placed from the keyboard. */
export const NEW_SHAPE = { width: 120, height: 80 };
/** a picture placed with a click is two inches wide. */
export const NEW_PICTURE_WIDTH = 144;

export class AnnotateSession {
  // declared first: the derived values below read them
  #host: AnnotateHost;
  #revision = -1;
  #tools: Record<string, PageTool>;

  tool = $state<AnnotateToolId>("select");
  selection = $state<TextSelection | null>(null);
  annotations = $state<AnnotationInfo[]>([]);
  selectedId = $state<string | null>(null);
  /** where the selected object's frame is drawn while it is dragged or nudged */
  framePreview = $state<Rect | null>(null);
  note = $state<NoteDraft | null>(null);
  /** the words in the open note's field, kept by NotePopover as they are typed */
  noteTyped = $state<string | null>(null);
  editing = $state<TextBoxDraft | null>(null);
  styles = $state<AnnotateStyles>(defaultStyles());
  /** the picture chosen for the Picture tool, with its width over height */
  picture = $state<{ path: string; aspect: number } | null>(null);
  /** a press is under way on a page */
  dragging = $state(false);
  /**
   * counts the times the person picked an annotation (a click with Select,
   * or a comment in the list). the tab opens the Selection section then,
   * but not when a tool selects what it just made, so drawing several
   * shapes in a row keeps the Shapes section open.
   */
  picks = $state(0);
  readonly texts: PageTextStore;

  readonly selected = $derived(this.annotations.find((annotation) => annotation.id === this.selectedId) ?? null);
  readonly pageTool: PageTool = $derived(this.#toolFor(this.tool, this.styles.shape.kind));

  constructor(host: AnnotateHost) {
    this.#host = host;
    this.texts = new PageTextStore((page) => pageText(host.documentId(), page));
    this.#tools = this.#makeTools();
  }

  // ---- the page tools ----

  #textHost(): TextHost {
    return {
      text: (page) => this.texts.get(page),
      selection: () => this.selection,
      setSelection: (selection) => (this.selection = selection),
      currentPage: () => this.#host.currentPage(),
      pageCount: () => this.#host.pageCount(),
    };
  }

  #makeTools(): Record<string, PageTool> {
    const host = this.#textHost();
    const pageSize = (page: number) => this.#host.pageSize(page);
    const outline: MarkStyle = { stroke: "rgb(0 136 255)", fill: null, width: 1, opacity: 1, dash: [4, 3] };
    const markTool = (kind: TextMarkupKind) => textTool({ id: kind, host, onSelected: (selection) => this.applyMark(kind, selection) });
    return {
      select: textTool({ id: "select", host, pick: (pointer) => this.#pick(pointer) }),
      highlight: markTool("highlight"),
      underline: markTool("underline"),
      strikeOut: markTool("strikeOut"),
      note: pointTool({ id: "note", onPlace: (page, point) => this.startNote(page, point) }),
      textBox: rectangleTool({
        id: "textBox",
        style: () => outline,
        pageSize,
        clickSize: () => NEW_TEXT_BOX,
        minSize: 12,
        onDraw: (page, rect) => this.startTextBox(page, rect),
      }),
      square: rectangleTool({ id: "shape-square", style: () => this.#shapeStyle(), pageSize, squareWithShift: true, onDraw: (page, rect) => this.addShape(page, rect) }),
      circle: rectangleTool({
        id: "shape-circle",
        shape: "ellipse",
        style: () => this.#shapeStyle(),
        pageSize,
        squareWithShift: true,
        onDraw: (page, rect) => this.addShape(page, rect),
      }),
      line: lineTool({ id: "shape-line", style: () => this.#shapeStyle(), arrow: () => false, pageSize, onDraw: (page, a, b) => this.addLine(page, a, b) }),
      arrow: lineTool({ id: "shape-arrow", style: () => this.#shapeStyle(), arrow: () => true, pageSize, onDraw: (page, a, b) => this.addLine(page, a, b) }),
      draw: inkTool({
        id: "draw",
        style: () => {
          const ink = this.#inkLook();
          return { style: { stroke: cssColor(ink.color), fill: null, width: ink.width, opacity: ink.opacity }, key: JSON.stringify(ink) };
        },
        onDrawing: (page, strokes) => this.addInk(page, strokes),
      }),
      picture: rectangleTool({
        id: "picture",
        style: () => outline,
        pageSize,
        aspect: () => this.picture?.aspect ?? null,
        clickSize: () => (this.picture ? { width: NEW_PICTURE_WIDTH, height: NEW_PICTURE_WIDTH / this.picture.aspect } : null),
        minSize: 6,
        onDraw: (page, rect) => this.addPicture(page, rect),
      }),
    };
  }

  #toolFor(tool: AnnotateToolId, shape: ShapeChoice): PageTool {
    return tool === "shape" ? this.#tools[shape] : this.#tools[tool];
  }

  #shapeStyle(): MarkStyle {
    const shape = this.styles.shape;
    return { stroke: cssColor(shape.color), fill: shape.fill ? cssColor(shape.fill) : null, width: shape.width, opacity: shape.opacity };
  }

  /** the pen Draw uses: the chosen one, or the highlighter preset. */
  #inkLook(): { color: Rgb; width: number; opacity: number } {
    const ink = this.styles.ink;
    return ink.highlighter ? HIGHLIGHTER : ink;
  }

  /** makes a tool the active one. the one before saves anything still waiting (a drawing). */
  setTool(tool: AnnotateToolId) {
    if (tool === this.tool) return;
    this.pageTool.finish?.();
    this.tool = tool;
    // a text selection is kept for the mark tools, which can apply it
    if (tool !== "select" && tool !== "highlight" && tool !== "underline" && tool !== "strikeOut") this.selection = null;
    if (tool !== "select") this.deselect();
  }

  /** the press on a page is about an annotation (Select tool), or null to select text. */
  #pick(pointer: PagePointer): Gesture | null {
    const hit = annotationAt(this.annotations, pointer.page, pointer);
    // a click away from the open note keeps what was typed in it, as
    // leaving a text box does; only Escape or the close button drop it
    if (this.note && hit?.id !== this.note.id) this.keepTypedNote();
    if (!hit) {
      this.deselect();
      this.note = null;
      return null;
    }
    this.selection = null;
    this.select(hit);
    this.picks += 1;
    if (hit.kind === "note") this.openNote(hit);
    if (pointer.clicks === 2 && hit.details.type === "textBox") this.editTextBox(hit);
    const { movable } = frameBehaviour(hit);
    const startRect = hit.rect;
    const size = this.#host.pageSize(hit.page);
    let moved: Rect | null = null;
    return {
      move: (now) => {
        if (!movable || !size) return;
        moved = moveFrame(startRect, now.x - pointer.x, now.y - pointer.y, size);
        this.framePreview = moved;
      },
      up: () => {
        if (moved && !sameRect(moved, startRect)) this.moveSelected(moved);
        else this.framePreview = null;
      },
      cancel: () => (this.framePreview = null),
    };
  }

  // ---- what is shown ----

  /** after every change: the text and annotations are read again for the new version. */
  setRevision(revision: number) {
    if (revision === this.#revision) return;
    this.#revision = revision;
    this.texts.setRevision(revision);
    this.reloadAnnotations();
  }

  reloadAnnotations() {
    const revision = this.#revision;
    listAllAnnotations(this.#host.documentId())
      .then((list) => {
        if (revision !== this.#revision) return;
        this.annotations = list;
        if (this.selectedId && !list.some((annotation) => annotation.id === this.selectedId)) this.deselect();
        this.framePreview = null;
      })
      .catch(() => {
        // a document whose annotations can't be read just shows none
        if (revision === this.#revision) this.annotations = [];
      });
  }

  /** goes to an annotation on its page and selects it; a note opens. */
  reveal(annotation: AnnotationInfo) {
    this.setTool("select");
    this.selection = null;
    this.#host.reveal(annotation.page, annotation.rect);
    this.select(annotation);
    this.picks += 1;
    if (annotation.kind === "note") this.openNote(annotation);
  }

  select(annotation: AnnotationInfo) {
    this.selectedId = annotation.id;
    this.framePreview = null;
  }

  deselect() {
    this.selectedId = null;
    this.framePreview = null;
  }

  /**
   * escape: drops whatever is most in the way, one thing per press. a
   * note or text box being written, then a tool other than Select, then
   * the selected object, then the text selection. false when nothing was left.
   */
  escape(): boolean {
    if (this.note) {
      this.note = null;
    } else if (this.editing) {
      this.editing = null;
    } else if (this.tool !== "select") {
      this.setTool("select");
    } else if (this.selectedId) {
      this.deselect();
    } else if (this.selection) {
      this.selection = null;
    } else {
      return false;
    }
    return true;
  }

  /** the selected words, for copying. */
  selectedWords(): string {
    return this.selection ? selectedText(this.selection, (page) => this.texts.get(page)) : "";
  }

  // ---- making changes ----

  /**
   * one edit through the viewer's queue. resolves to what the engine made
   * or changed, or null. `replacing` is the id it had before: an
   * annotation from another app is called "ref:..." until its first
   * change gives it a lasting id.
   */
  async #edit(action: () => Promise<AnnotationEdit>, replacing?: string): Promise<AnnotationInfo | null> {
    let made: AnnotationInfo | null = null;
    const worked = await this.#host.change(async () => {
      const edit = await action();
      made = edit.annotation;
      return edit.document;
    });
    if (!worked || !made) return null;
    const result: AnnotationInfo = made;
    // shown straight away; the full list is read again for the new revision
    const gone = new Set([result.id, replacing]);
    this.annotations = [...this.annotations.filter((annotation) => !gone.has(annotation.id)), result];
    if (replacing && this.selectedId === replacing) this.selectedId = result.id;
    return result;
  }

  /** adds something, selects it if asked, and says so to screen readers. */
  async #add(action: () => Promise<AnnotationEdit>, select: boolean): Promise<AnnotationInfo | null> {
    const made = await this.#edit(action);
    if (!made) return null;
    if (select) this.select(made);
    this.#host.announce(addedAnnouncement(kindLabel(made), made.page));
    return made;
  }

  /** highlights, underlines or strikes through the selection, one undo step for every page it covers. */
  async applyMark(kind: TextMarkupKind, selection: TextSelection | null = this.selection): Promise<boolean> {
    if (!selection) return false;
    const marks = selectedPages(selection)
      .map((page) => {
        const text = this.texts.get(page);
        return { page, rects: text ? selectionRects(selection, page, text) : [] };
      })
      .filter((mark) => mark.rects.length > 0)
      .map(({ page, rects }) => ({ page, params: { kind, rects, color: this.styles.marks[kind], opacity: 1 } }));
    if (marks.length === 0) return false;
    const id = this.#host.documentId();
    let made: AnnotationInfo[] = [];
    const worked = await this.#host.change(async () => {
      const edit = await addTextMarkups(id, marks);
      made = edit.annotations;
      return edit.document;
    });
    if (!worked) return false;
    this.selection = null;
    const kept = new Set(made.map((annotation) => annotation.id));
    this.annotations = [...this.annotations.filter((annotation) => !kept.has(annotation.id)), ...made];
    const first = made[0];
    if (first) this.#host.announce(addedAnnouncement(kindLabel(first), first.page));
    return true;
  }

  /** opens the note pop-up for a new note centred on a point. */
  startNote(page: number, point: Point) {
    const size = this.#host.pageSize(page);
    const corner = size ? rectAround(point, NOTE_ICON_SIZE, NOTE_ICON_SIZE, size) : { x: point.x, y: point.y };
    this.deselect();
    this.note = { page, position: { x: corner.x, y: corner.y }, id: null, text: "" };
    // a note is placed one at a time; Select comes back for reading it
    this.tool = "select";
  }

  openNote(annotation: AnnotationInfo) {
    this.note = { page: annotation.page, position: { x: annotation.rect.x, y: annotation.rect.y }, id: annotation.id, text: annotation.contents ?? "" };
  }

  /** saves the note being written: adds a new one, or changes the comment of one that exists. */
  async saveNote(text: string, draft: NoteDraft | null = this.note): Promise<boolean> {
    if (!draft) return false;
    const id = this.#host.documentId();
    // the pop-up shows the saved note only if it is still the open one
    const stillOpen = () => this.note === draft;
    if (draft.id === null) {
      const made = await this.#add(() => addNote(id, draft.page, { position: draft.position, color: this.styles.note, contents: text }), true);
      if (made && stillOpen()) this.note = { ...draft, id: made.id, text };
      return made !== null;
    }
    const noteId = draft.id;
    if (text === draft.text) return true;
    const changed = await this.#edit(() => updateAnnotation(id, draft.page, noteId, { contents: text }));
    if (changed && stillOpen()) this.note = { ...draft, text };
    return changed !== null;
  }

  /**
   * the note is being closed by a click elsewhere: what was typed in it is
   * saved, unless nothing changed or a new note was left empty.
   */
  keepTypedNote() {
    const draft = this.note;
    const typed = this.noteTyped;
    if (!draft || typed === null || typed === draft.text) return;
    if (draft.id === null && typed.trim() === "") return;
    void this.saveNote(typed, draft);
  }

  async reply(to: AnnotationInfo, text: string): Promise<boolean> {
    const id = this.#host.documentId();
    const made = await this.#edit(() => addReply(id, to.page, to.id, text));
    if (made) this.#host.announce(`Reply added to page ${to.page + 1}.`);
    return made !== null;
  }

  /** changes the comment of any annotation or reply. */
  async editComment(annotation: AnnotationInfo, text: string): Promise<boolean> {
    const id = this.#host.documentId();
    return (await this.#edit(() => updateAnnotation(id, annotation.page, annotation.id, { contents: text }))) !== null;
  }

  /** opens a text box for typing, on the page, where it will be. */
  startTextBox(page: number, rect: Rect) {
    this.deselect();
    this.editing = { page, rect, id: null, text: "" };
    this.tool = "select";
  }

  editTextBox(annotation: AnnotationInfo) {
    if (annotation.details.type !== "textBox" || !annotation.editable) return;
    this.keepTypedNote();
    this.note = null;
    // the tab's text controls show, and change, this box's own style while it is typed in
    const { font, fontSize, textColor, align, fill } = annotation.details;
    this.styles.textBox = { ...fontParts(font), size: fontSize, color: textColor, align, fill };
    this.editing = { page: annotation.page, rect: annotation.rect, id: annotation.id, text: annotation.details.text };
  }

  /** turns the text box being typed into an annotation, or changes the one being edited. */
  async commitTextBox(text: string, rect: Rect): Promise<boolean> {
    const draft = this.editing;
    if (!draft) return false;
    this.editing = null;
    const id = this.#host.documentId();
    const style = this.styles.textBox;
    const font = standardFont(style.family, style.bold, style.italic);
    if (draft.id === null) {
      if (text.trim() === "") return false;
      const params = { rect, text, font, fontSize: style.size, color: style.color, align: style.align, border: null, fill: style.fill, opacity: 1 };
      return (await this.#add(() => addTextBox(id, draft.page, params), true)) !== null;
    }
    const boxId = draft.id;
    if (text.trim() === "") return this.deleteById(draft.page, boxId);
    const changes: AnnotationChanges = { text, font, fontSize: style.size, color: style.color, align: style.align, fill: style.fill };
    if (!sameRect(rect, draft.rect)) changes.rect = rect;
    return (await this.#edit(() => updateAnnotation(id, draft.page, boxId, changes))) !== null;
  }

  async addShape(page: number, rect: Rect): Promise<AnnotationInfo | null> {
    const shape = this.styles.shape;
    const kind = shape.kind === "circle" ? "circle" : "square";
    const id = this.#host.documentId();
    const params: ShapeSettings = { kind, rect, stroke: { color: shape.color, width: shape.width, dash: [] }, fill: shape.fill, opacity: shape.opacity };
    return this.#add(() => addShape(id, page, params), true);
  }

  async addLine(page: number, start: Point, end: Point): Promise<AnnotationInfo | null> {
    const shape = this.styles.shape;
    const id = this.#host.documentId();
    const params = {
      start,
      end,
      stroke: { color: shape.color, width: shape.width, dash: [] },
      startEnding: "none" as const,
      endEnding: shape.kind === "arrow" ? ("openArrow" as const) : ("none" as const),
      opacity: shape.opacity,
    };
    return this.#add(() => addLine(id, page, params), true);
  }

  async addInk(page: number, strokes: Point[][]): Promise<AnnotationInfo | null> {
    const ink = this.#inkLook();
    const id = this.#host.documentId();
    const saved = strokes.map((stroke) => (stroke.length === 1 ? [stroke[0], { x: stroke[0].x + 0.01, y: stroke[0].y + 0.01 }] : stroke));
    return this.#add(() => addInk(id, page, { strokes: saved, color: ink.color, width: ink.width, opacity: ink.opacity }), false);
  }

  async addPicture(page: number, rect: Rect): Promise<AnnotationInfo | null> {
    const picture = this.picture;
    if (!picture) return null;
    const id = this.#host.documentId();
    return this.#add(() => addImage(id, page, picture.path, { rect, opacity: 1 }), true);
  }

  /** puts a note, text box, shape or picture in the middle of the view, for the keyboard. */
  async placeFromKeyboard(tool: "note" | "textBox" | "shape" | "picture") {
    const centre = this.#host.viewCentre();
    const size = this.#host.pageSize(centre.page);
    if (!size) return;
    if (tool === "note") return this.startNote(centre.page, centre);
    if (tool === "textBox") return this.startTextBox(centre.page, rectAround(centre, NEW_TEXT_BOX.width, NEW_TEXT_BOX.height, size));
    if (tool === "picture") {
      if (!this.picture) return;
      return this.addPicture(centre.page, rectAround(centre, NEW_PICTURE_WIDTH, NEW_PICTURE_WIDTH / this.picture.aspect, size));
    }
    const kind = this.styles.shape.kind;
    if (kind === "line" || kind === "arrow") {
      return this.addLine(centre.page, { x: centre.x - NEW_SHAPE.width / 2, y: centre.y }, { x: centre.x + NEW_SHAPE.width / 2, y: centre.y });
    }
    return this.addShape(centre.page, rectAround(centre, NEW_SHAPE.width, NEW_SHAPE.height, size));
  }

  /** moves or resizes the selected annotation to `rect`. */
  async moveSelected(rect: Rect): Promise<boolean> {
    const selected = this.selected;
    if (!selected) return false;
    this.framePreview = rect;
    const changed = await this.restyle({ rect });
    if (!changed) this.framePreview = null;
    return changed;
  }

  /** changes the selected annotation: its place, colour, width, fill, opacity or text style. */
  async restyle(changes: AnnotationChanges): Promise<boolean> {
    const selected = this.selected;
    if (!selected) return false;
    const id = this.#host.documentId();
    const changed = await this.#edit(() => updateAnnotation(id, selected.page, selected.id, changes), selected.id);
    return changed !== null;
  }

  async deleteSelected(): Promise<boolean> {
    const selected = this.selected;
    if (!selected) return false;
    return this.deleteById(selected.page, selected.id);
  }

  /** deletes an annotation with its replies, and says so. */
  async deleteById(page: number, annotationId: string): Promise<boolean> {
    const id = this.#host.documentId();
    const found = this.annotations.find((annotation) => annotation.id === annotationId);
    const worked = await this.#host.change(() => deleteAnnotation(id, page, annotationId));
    if (!worked) return false;
    this.annotations = this.annotations.filter((annotation) => annotation.id !== annotationId && annotation.replyTo !== annotationId);
    if (this.selectedId === annotationId) this.deselect();
    if (this.note?.id === annotationId) this.note = null;
    this.#host.announce(`${found ? kindLabel(found) : "Comment"} deleted from page ${page + 1}.`);
    return true;
  }
}

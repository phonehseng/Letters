// what the Edit tab's tools share while a document is open: the page
// tool (Edit Text and Images, Add Text, Add Image), the text and pictures
// of the pages on screen, the paragraph being typed in on the page, and
// the notice shown after an edit. the pictures have their own half,
// `pictures` (pictureTools.svelte.ts), as do the shapes (`shapes`), text
// that re-wraps and pushes the column down (`flow`, textFlow.svelte.ts)
// and the question before changing a signed file (`consent`).
//
// every change goes through the viewer's change queue, so each is one
// undo step, and through `consent` first. the engine answers the changed
// page's objects with fresh ids, which are kept at once: ids are only
// valid until the next edit.

import type { DocumentState } from "../api";
import type { Dispatcher } from "../commands";
import type { PageSize } from "../layout";
import type { PagePoint, Point, Rect } from "../pageTools/pagePoints";
import type { MarkStyle, PageTool } from "../pageTools/pageTool";
import { pointTool, rectangleTool } from "../pageTools/tools.svelte";
import type { ToolName } from "../tools";
import { caretIndexFor, leadingRun, matchingBlock } from "./blocks";
import { extensionSentence, summarySentence } from "./changeSentences";
import { ConsentGate } from "./consent.svelte";
import { embeddedFace, lookOf, measureMetrics, nearestStandard, standardLook, type CssFontLook } from "./cssFonts";
import { contentApi, type ChangesSummary } from "./contentApi";
import { editApi, type AddTextFont, type PageObjects, type Rgb, type StandardFont, type TextBlock, type TextEditAnswer, type TextStyleChange } from "./editApi";
import { EDIT_COMMANDS, type EditCommand } from "./editCommands";
import { editedAnnouncement, noticeSentence, overflowSentence } from "./editSentences";
import type { FontMetrics } from "./editorGeometry";
import { planFlow } from "./flowPlan";
import { PageObjectsStore } from "./pageObjectsStore.svelte";
import { PageSetup } from "./pageSetup.svelte";
import { PictureTools } from "./pictureTools.svelte";
import { ShapeTools } from "./shapeTools.svelte";
import { TextFlow } from "./textFlow.svelte";
import { clearsBlock, editorText, planTextChange, type TextChange } from "./textDiff";

export type EditToolId = "none" | "edit" | "addText" | "addImage";

export interface EditHost {
  documentId: () => number;
  pageSize: (page: number) => PageSize | undefined;
  currentPage: () => number;
  viewCentre: () => PagePoint;
  /** runs a change through the window's queue; false when it failed (the error is already shown) */
  change: (action: () => Promise<DocumentState>) => Promise<boolean>;
  /** Undo, through the same queue */
  undo: () => Promise<boolean>;
  announce: (text: string) => void;
  notify: (text: string, action?: { label: string; run: () => void }) => void;
  showError: (message: string) => void;
  focusDocument: () => void;
  openTool: (tool: ToolName) => void;
  /** the other page tools let go: an Edit tool is taking over */
  takePageTool: () => void;
}

/** the paragraph (or new text) typed in on the page. */
export interface EditorDraft {
  page: number;
  /** null while adding new text */
  block: TextBlock | null;
  /** where new text starts (top left), or the block's own corner */
  position: Point;
  /** the text the editor opened with */
  original: string;
  /** what is typed now, kept in step by the editor */
  text: string;
  /** where the caret starts */
  caret: number;
  /** the selection inside the editor, for the style controls */
  selection: [number, number];
  look: CssFontLook;
  metrics: FontMetrics;
}

/** what is said beside text after an edit. */
export interface EditNotice {
  page: number;
  /** the text it is about, to place the notice under it */
  rect: Rect;
  sentences: string[];
  /** set when the new text grew past its paragraph: Let It Grow or Shrink to Fit */
  overflow: { block: Rect; text: string } | null;
}

/** what new text is written in. */
export interface NewTextStyle {
  font: AddTextFont;
  /** the name shown in the menu */
  label: string;
  size: number;
  color: Rgb;
}

/** a selection inside the editor that lies within one line, as that line's own range. */
export interface LineRange {
  lineId: string;
  range: [number, number];
}

export const BLACK: Rgb = { r: 0, g: 0, b: 0 };

/** how long the editor waits for the page's own font before it opens with the closest one. */
const FACE_WAIT_MS = 400;

/** the dashed outline of a picture being placed: the accent, as for a selection. */
const PLACING: MarkStyle = { stroke: "rgb(0 136 255)", fill: null, width: 1, opacity: 1, dash: [4, 3] };

export class EditSession {
  // declared first: the derived values below read them
  #host: EditHost;
  #tools: Record<Exclude<EditToolId, "none">, PageTool>;
  /** the revision the session's own last edit made, so it isn't mistaken for someone else's */
  #ownRevision = -1;

  tool = $state<EditToolId>("none");
  readonly objects: PageObjectsStore;
  readonly pictures: PictureTools;
  readonly shapes: ShapeTools;
  readonly flow: TextFlow;
  readonly consent: ConsentGate;
  readonly pageSetup: PageSetup;
  editor = $state<EditorDraft | null>(null);
  notice = $state<EditNotice | null>(null);
  newText = $state<NewTextStyle>({ font: { type: "standard", font: "helvetica" }, label: "Helvetica", size: 12, color: BLACK });
  /** Add Text writes in the style of the paragraph nearby (its font, size, spacing and colour) */
  matchNearby = $state(true);

  readonly pageTool: PageTool | null = $derived(this.#toolFor(this.tool));

  constructor(host: EditHost) {
    this.consent = new ConsentGate(host.documentId);
    // every change of the Edit tab asks first when the file is signed
    const change = (action: () => Promise<DocumentState>) => this.consent.guarded(() => host.change(action));
    this.#host = { ...host, change };
    this.objects = new PageObjectsStore((page) => editApi.pageObjects(host.documentId(), page));
    const shared = {
      ...this.#host,
      objects: this.objects,
      consent: this.consent,
      keep: (revision: number, objects: PageObjects, stale: boolean) => this.#keep(revision, objects, stale),
      finishTyping: () => this.#finishTyping(),
    };
    // a picture and a shape are never selected at once
    this.pictures = new PictureTools({
      ...shared,
      finishTyping: () => this.#finishTyping(this.shapes),
      useTool: (tool) => this.setTool(tool),
    });
    this.shapes = new ShapeTools({ ...shared, finishTyping: () => this.#finishTyping(this.pictures) });
    this.pageSetup = new PageSetup(shared);
    this.flow = new TextFlow({
      ...shared,
      showNotice: (page, rect, sentences) => (this.notice = { page, rect, sentences, overflow: null }),
      reopen: (page, block, text) => this.#reopen(page, block, text),
    });
    this.#tools = this.#makeTools();
  }

  // ---- the page tools ----

  #makeTools(): Record<Exclude<EditToolId, "none">, PageTool> {
    return {
      // the paragraphs and pictures are their own targets on the page
      // (EditOverlay), so a press that reaches the tool landed on neither
      edit: { id: "edit-content", cursor: "default", crossesPages: false, down: () => this.#pressedOutside(), marks: () => [] },
      addText: pointTool({ id: "add-page-text", cursor: "text", onPlace: (page, point) => this.startNewText(page, point) }),
      addImage: rectangleTool({
        id: "add-page-image",
        cursor: "crosshair",
        style: () => PLACING,
        pageSize: (page) => this.#host.pageSize(page),
        aspect: () => this.pictures.chosen?.aspect ?? null,
        clickSize: () => this.pictures.clickSize(),
        minSize: 6,
        onDraw: (page, rect) => this.pictures.placeChosen(page, rect),
      }),
    };
  }

  #toolFor(tool: EditToolId): PageTool | null {
    return tool === "none" ? null : this.#tools[tool];
  }

  setTool(tool: EditToolId) {
    if (tool === this.tool) return;
    if (tool !== "none") this.#host.takePageTool();
    this.#closeEditorQuietly();
    this.tool = tool;
    if (tool !== "edit") {
      this.pictures.deselect();
      this.shapes.deselect();
    }
  }

  /** another tool took over the pages. */
  dropTool() {
    if (this.tool !== "none") this.setTool("none");
  }

  /** a press on the page that missed every paragraph and picture: finish up, select nothing. */
  #pressedOutside(): null {
    this.#finishTyping();
    this.pictures.deselect();
    this.shapes.deselect();
    return null;
  }

  /** the paragraph being typed in becomes part of the page, the last notice goes, and `other` lets go. */
  #finishTyping(other?: { deselect: () => void }) {
    if (this.editor) void this.commitEditor();
    this.notice = null;
    other?.deselect();
  }

  /** escape: drops the notice, the editor, the selected picture, then the tool. false when nothing was left. */
  escape(): boolean {
    if (this.notice) this.notice = null;
    else if (this.editor) this.editor = null;
    else if (this.pictures.cropping) this.pictures.cancelCrop();
    else if (this.pictures.selected) this.pictures.deselect();
    else if (this.shapes.selected) this.shapes.deselect();
    else if (this.tool !== "none") this.setTool("none");
    else return false;
    return true;
  }

  /** after every change of the document: the pages are read again. */
  setRevision(revision: number) {
    this.objects.setRevision(revision);
    // undo, redo or another tool changed the page under the editor: its ids are gone
    if (revision !== this.#ownRevision && this.editor?.block) this.editor = null;
    this.pictures.preview = null;
    this.shapes.preview = null;
    this.shapes.pointPreview = null;
  }

  /** the menu bar and the keys reach the Edit tools through the window's dispatcher. */
  attach(commands: Dispatcher): () => void {
    const handlers: Record<EditCommand, () => void> = {
      "edit-text": () => {
        this.#host.openTool("editText");
        this.setTool(this.tool === "edit" ? "none" : "edit");
      },
      "add-page-text": () => {
        this.#host.openTool("addPageText");
        this.setTool("addText");
      },
      "add-page-image": () => this.#host.openTool("addPageImage"),
    };
    const takeBack = EDIT_COMMANDS.map((command) => commands.register(command, handlers[command]));
    return () => takeBack.forEach((unregister) => unregister());
  }

  // ---- typing in place ----

  /** opens the editor over a block, with the caret where it was clicked; `typed` starts it with text typed before. */
  async openBlock(page: number, block: TextBlock, at: Point | null, typed: string | null = null) {
    if (this.editor) await this.commitEditor();
    this.pictures.deselect();
    this.notice = null;
    const look = await this.#lookOf(block);
    const original = editorText(block);
    const text = typed ?? original;
    const caret = at ? caretIndexFor(block, at) : text.length;
    const position = { x: block.rect.x, y: block.rect.y };
    const metrics = measureMetrics(look);
    this.editor = { page, block, position, original, text, caret, selection: [caret, caret], look, metrics };
  }

  /** the page's own font when the web view can load it, else the closest; never waits long for it. */
  async #lookOf(block: TextBlock): Promise<CssFontLook> {
    const run = leadingRun(block);
    if (!run) return standardLook("helvetica");
    const loading = embeddedFace(this.#host.documentId(), run.font, editorText(block));
    const face = await Promise.race([loading, wait(FACE_WAIT_MS)]);
    return lookOf(run.font, face);
  }

  /** opens an empty editor where new text starts. */
  startNewText(page: number, point: Point) {
    this.pictures.deselect();
    this.notice = null;
    const look = this.#newTextLook();
    const metrics = measureMetrics(look);
    this.editor = { page, block: null, position: point, original: "", text: "", caret: 0, selection: [0, 0], look, metrics };
    // new text is placed one at a time; the next click can edit it
    this.tool = "edit";
  }

  #newTextLook(): CssFontLook {
    const font = this.newText.font;
    if (font.type === "standard") return standardLook(font.font);
    return { family: `"${this.newText.label}", sans-serif`, weight: 400, style: "normal" };
  }

  /** the editor's text changed. */
  typed(text: string, selection: [number, number]) {
    if (!this.editor) return;
    this.editor.text = text;
    this.editor.selection = selection;
  }

  /** Escape in the editor: nothing changes. */
  cancelEditor() {
    this.editor = null;
    this.#host.focusDocument();
  }

  #closeEditorQuietly() {
    if (this.editor && this.editor.text !== this.editor.original) void this.commitEditor();
    else this.editor = null;
  }

  /** makes what was typed part of the page, as one undo step. */
  async commitEditor(): Promise<boolean> {
    const draft = this.editor;
    if (!draft) return false;
    this.editor = null;
    if (!draft.block) return this.#addNewText(draft);
    // a paragraph that no longer fits its lines re-wraps and moves what is below
    const plan = planFlow(draft.block, draft.text);
    if (plan.kind === "none") return true;
    if (plan.kind === "reflow") return this.flow.reflow(draft.page, draft.block, plan.text);
    return this.#applyChange(draft.page, draft.block, plan, draft.text, true);
  }

  /** the overflow sheet's Cancel: the editor opens again with what was typed. */
  #reopen(page: number, block: TextBlock, text: string) {
    const fresh = matchingBlock(this.objects.get(page) ?? { page, width: 0, height: 0, textBlocks: [], images: [] }, block.rect) ?? block;
    void this.openBlock(page, fresh, null, text);
  }

  async #addNewText(draft: EditorDraft): Promise<boolean> {
    if (draft.text.trim() === "") return false;
    if (this.matchNearby) return this.#addMatchingText(draft);
    const id = this.#host.documentId();
    const style = this.newText;
    const answer = await this.#runText(() => editApi.addText(id, draft.page, draft.position, draft.text, style.font, style.size, style.color));
    if (!answer) return false;
    this.#host.announce(`Text added to page ${draft.page + 1}.`);
    return true;
  }

  /** new text in the font, size, spacing and colour of the paragraph nearby, adding letters to its font as needed. */
  async #addMatchingText(draft: EditorDraft): Promise<boolean> {
    const id = this.#host.documentId();
    const added = { position: draft.position, text: draft.text.replace(/\n+/g, " "), width: null, likeBlock: null };
    let summary: ChangesSummary | null = null;
    const worked = await this.#host.change(async () => {
      const answer = await contentApi.addMatchingText(id, draft.page, added, this.consent.consent);
      summary = answer.summary;
      this.#keep(answer.document.revision, answer.objects, answer.stale);
      return answer.document;
    });
    if (!worked || !summary) return false;
    const made: ChangesSummary = summary;
    const sentences = made.fontsExtended.map(extensionSentence);
    if (sentences.length > 0) this.notice = { page: draft.page, rect: { x: draft.position.x, y: draft.position.y, width: 1, height: 12 }, sentences, overflow: null };
    // the engine's note says whose style was matched ("Matched the style of …")
    const quiet = summarySentence(made);
    if (quiet) this.#host.notify(quiet);
    this.#host.announce(`Text added to page ${draft.page + 1} in the style of the text nearby.`);
    return true;
  }

  /** runs one planned change; a stale id finds the block again and tries once more. */
  async #applyChange(page: number, block: TextBlock, change: TextChange, typed: string, retry: boolean): Promise<boolean> {
    const answer = await this.#runText(() => this.#callFor(page, change));
    if (!answer) return false;
    if (!answer.stale) {
      this.#afterTextEdit(page, block, answer, change);
      return true;
    }
    const again = matchingBlock(answer.objects, block.rect);
    // only the same words may be changed again: a paragraph that changed meanwhile is left alone
    const sameWords = again !== null && editorText(again) === editorText(block);
    if (!retry || !again || !sameWords) {
      this.#host.notify("That text changed while you were typing, so nothing was changed. Click it to edit it again.");
      return false;
    }
    return this.#applyChange(page, again, planTextChange(again, typed), typed, false);
  }

  #callFor(page: number, change: TextChange): Promise<TextEditAnswer> {
    const id = this.#host.documentId();
    switch (change.kind) {
      case "insert":
        return editApi.insertText(id, page, change.lineId, change.index, change.text);
      case "delete":
        return editApi.deleteChars(id, page, change.lineId, change.start, change.end);
      case "replaceLine":
        return editApi.replaceText(id, page, change.lineId, change.text);
      case "replaceBlock":
        if (clearsBlock(change)) return editApi.deleteText(id, page, change.blockId);
        return editApi.replaceText(id, page, change.blockId, change.text);
      case "none":
        return Promise.reject(new Error("nothing to change"));
    }
  }

  /** one text edit through the queue; the answer, or null when it failed (already shown). */
  async #runText(call: () => Promise<TextEditAnswer>): Promise<TextEditAnswer | null> {
    let answer: TextEditAnswer | null = null;
    const worked = await this.#host.change(async () => {
      answer = await call();
      // kept before the viewer shows the new revision, so the page's
      // fresh objects are there when it asks, and an editor opened
      // meanwhile on another paragraph isn't taken for out of date
      this.#keep(answer.document.revision, answer.objects, answer.stale);
      return answer.document;
    });
    return worked ? answer : null;
  }

  #keep(revision: number, objects: PageObjects, stale: boolean) {
    if (!stale) this.#ownRevision = revision;
    this.objects.accept(revision, objects);
  }

  #afterTextEdit(page: number, block: TextBlock, answer: TextEditAnswer, change: TextChange) {
    const sentences = answer.notices.map(noticeSentence);
    const overflow = answer.overflow;
    if (overflow) sentences.push(overflowSentence(overflow));
    if (sentences.length > 0) {
      const now = matchingBlock(answer.objects, block.rect);
      // only a re-set paragraph can be shrunk back into its lines
      const canShrink = overflow !== null && change.kind === "replaceBlock";
      this.notice = {
        page,
        rect: now?.rect ?? block.rect,
        sentences,
        overflow: canShrink ? { block: block.rect, text: change.text } : null,
      };
    }
    this.#host.announce(editedAnnouncement(page, answer.notices));
  }

  /** the overflow notice's Let It Grow: keep the text as it is. */
  letItGrow() {
    this.notice = null;
  }

  /** the overflow notice's Shrink to Fit: undo, then put the text in the paragraph's own lines, smaller. */
  async shrinkToFit(): Promise<boolean> {
    const notice = this.notice;
    if (!notice?.overflow) return false;
    this.notice = null;
    if (!(await this.#host.undo())) return false;
    const objects = await this.objects.refresh(notice.page);
    const block = matchingBlock(objects, notice.overflow.block);
    if (!block) return false;
    const id = this.#host.documentId();
    const text = notice.overflow.text;
    const answer = await this.#runText(() => editApi.replaceText(id, notice.page, block.id, text, true));
    if (!answer || answer.stale) return false;
    this.#afterTextEdit(notice.page, block, answer, { kind: "none" });
    return true;
  }

  /** the block under the editor, deleted: Delete Text. */
  async deleteParagraph(): Promise<boolean> {
    const draft = this.editor;
    const block = draft?.block;
    if (!draft || !block) return false;
    this.editor = null;
    const id = this.#host.documentId();
    const answer = await this.#runText(() => editApi.deleteText(id, draft.page, block.id));
    if (!answer || answer.stale) return false;
    this.#host.announce(`Text deleted from page ${draft.page + 1}.`);
    this.#host.focusDocument();
    return true;
  }

  /** the paragraph's size, colour or (standard) font: the selected letters of one line, or the whole paragraph. */
  async restyle(style: TextStyleChange): Promise<boolean> {
    const draft = this.editor;
    if (!draft?.block) return false;
    if (draft.text === draft.original) {
      this.editor = null;
      return this.#restyleBlock(draft.page, draft.block, rangeInOneLine(draft.block, draft.selection), style);
    }
    // what was typed is made first, so the style lands on the text as it is now
    const block = draft.block;
    if (!(await this.commitEditor())) return false;
    const fresh = matchingBlock(await this.objects.refresh(draft.page), block.rect);
    if (!fresh) return false;
    return this.#restyleBlock(draft.page, fresh, null, style);
  }

  async #restyleBlock(page: number, block: TextBlock, range: LineRange | null, style: TextStyleChange): Promise<boolean> {
    const id = this.#host.documentId();
    const target = range?.lineId ?? block.id;
    const answer = await this.#runText(() => editApi.setTextStyle(id, page, target, range?.range ?? null, style));
    if (!answer || answer.stale) return false;
    this.#afterTextEdit(page, block, answer, { kind: "none" });
    // the paragraph opens again, so more changes can follow
    const again = matchingBlock(answer.objects, block.rect);
    if (again) await this.openBlock(page, again, null);
    return true;
  }

  /** the standard font nearest the text being edited, for the font menu. */
  currentStandardFont(): StandardFont | null {
    const run = this.editor?.block ? leadingRun(this.editor.block) : null;
    return run ? nearestStandard(run.font) : null;
  }
}

/** a selection inside the editor that stays within one line, as that line's own range. */
export function rangeInOneLine(block: TextBlock, selection: [number, number]): LineRange | null {
  const start = Math.min(selection[0], selection[1]);
  const end = Math.max(selection[0], selection[1]);
  if (start === end) return null;
  let offset = 0;
  for (const line of block.lines) {
    const lineEnd = offset + line.text.length;
    if (start >= offset && end <= lineEnd) return { lineId: line.id, range: [start - offset, end - offset] };
    // the editor joins lines with one new line character
    offset = lineEnd + 1;
  }
  return null;
}

function wait(ms: number): Promise<null> {
  return new Promise((resolve) => setTimeout(() => resolve(null), ms));
}

// what the Protect tools share while a document is open: the marks for
// redaction (only in this window until applied), the Mark Text and Mark
// Area page tools, the review, the apply and close questions, and the
// document's password status.
//
// the viewer makes one of these beside the annotate session. its page
// tool is the active one while it has one (see `pageTool`); picking a
// Protect tool asks the other sessions to let go, and they ask this one.
// every change to the document goes through the viewer's change queue.

import type { DocumentState } from "../api";
import { pageText as fetchPageText } from "../annotate/annotateApi";
import type { Dispatcher } from "../commands";
import type { PageSize } from "../layout";
import { rectAround } from "../pageTools/gestures";
import type { PagePoint, Rect } from "../pageTools/pagePoints";
import type { MarkStyle, PageTool } from "../pageTools/pageTool";
import { isCollapsed, selectedPages, selectedText, selectionRects, type TextSelection } from "../pageTools/textSelection";
import { rectangleTool, textTool, type TextHost } from "../pageTools/tools.svelte";
import type { ToolName } from "../tools";
import { marksFromFound, type FoundItem } from "./findMarks";
import {
  addMarks,
  areaCount,
  asRedactionAreas,
  keepMatchingPages,
  markedPages,
  markIds,
  moveMark,
  padTextBox,
  pageSignature,
  removeMark,
  type NewMark,
  type PendingMark,
} from "./pendingMarks";
import {
  applyRedactions,
  changeLockOnSave,
  closeRedactionDryRun,
  protectionStatus,
  redactionDryRun,
  redactionPreview,
  unlockDocument,
  type DryRun,
  type LockChange,
  type ProtectionStatus,
  type RedactOptions,
  type RedactionPreview,
} from "./protectApi";
import { PROTECT_COMMANDS, type ProtectCommand } from "./protectCommands";
import { appliedMessage } from "./protectEffects";
import { protectGuard } from "./protectGuard";
import { reviewPages, reviewWarnings, type ReviewPage, type ReviewWarning } from "./review";

export type ProtectToolId = "markText" | "markArea";

/** what the viewer lends the session. */
export interface ProtectHost {
  documentId: () => number;
  pageSize: (page: number) => PageSize | undefined;
  pageCount: () => number;
  currentPage: () => number;
  viewCentre: () => PagePoint;
  /** runs a change through the window's queue; false when it failed (the error is already shown) */
  change: (action: () => Promise<DocumentState>) => Promise<boolean>;
  announce: (text: string) => void;
  reveal: (page: number, rect: Rect) => void;
  notify: (text: string, action?: { label: string; run: () => void }) => void;
  showError: (message: string) => void;
  focusDocument: () => void;
  openTool: (tool: ToolName) => void;
  /** the page text and the one shared text selection (the annotate session's) */
  text: TextHost;
  /** the other page tools let go: a Protect tool is taking over */
  takePageTool: () => void;
}

/** the review sheet while it is open. */
export interface ReviewState {
  loading: boolean;
  preview: RedactionPreview | null;
  dryRun: DryRun | null;
  pages: ReviewPage[];
  warnings: ReviewWarning[];
  /** the page shown before and after */
  shownPage: number;
  error: string | null;
}

/** a box placed from the keyboard. */
export const KEYBOARD_AREA = { width: 160, height: 36 };

/** the look of a mark while it is drawn: the same red outline as marks on the page. */
const DRAWING: MarkStyle = { stroke: "rgb(215 0 21)", fill: "rgb(215 0 21 / 0.12)", width: 1.5, opacity: 1, dash: [5, 3] };

export const BLACK = { r: 0, g: 0, b: 0 };
export const WHITE = { r: 255, g: 255, b: 255 };

export class ProtectSession {
  // declared first: the derived values below read them
  #host: ProtectHost;
  #tools: Record<ProtectToolId, PageTool>;
  #newId = markIds();
  /** what each marked page looked like when it was marked (see pageSignature) */
  #signatures = new Map<number, string>();
  #revision = -1;

  tool = $state<ProtectToolId | null>(null);
  marks = $state<PendingMark[]>([]);
  selectedMarkId = $state<string | null>(null);
  /** where the selected mark is drawn while it is dragged or nudged */
  framePreview = $state<Rect | null>(null);
  /** goes up when the selected mark's frame should take the focus */
  focusTick = $state(0);
  status = $state<ProtectionStatus | null>(null);
  review = $state<ReviewState | null>(null);
  /** the apply sheet is showing */
  confirmingApply = $state(false);
  options = $state<RedactOptions>({ fill: BLACK, overlayText: null, clipCrossingPaths: false });
  /** how many areas the last apply redacted, for "Also remove hidden information…" */
  lastApplied = $state(0);
  /** the question before marks would be lost, while it is showing */
  leaveQuestion = $state<{ count: number; answer: (discard: boolean) => void } | null>(null);
  /** a phrase the review asked Find and Mark to look for */
  findRequest = $state<{ phrase: string; at: number } | null>(null);

  readonly selectedMark = $derived(this.marks.find((mark) => mark.id === this.selectedMarkId) ?? null);
  readonly pageTool: PageTool | null = $derived(this.#toolFor(this.tool));

  constructor(host: ProtectHost) {
    this.#host = host;
    this.#tools = {
      markText: textTool({ id: "redact-text", host: host.text, onSelected: (selection) => this.markSelection(selection) }),
      markArea: rectangleTool({
        id: "redact-area",
        style: () => DRAWING,
        pageSize: (page) => host.pageSize(page),
        minSize: 2,
        onDraw: (page, rect) => this.markArea(page, rect),
      }),
    };
  }

  // ---- the page tools ----

  #toolFor(tool: ProtectToolId | null): PageTool | null {
    return tool ? this.#tools[tool] : null;
  }

  /** makes a Protect tool the active page tool, or none. */
  setTool(tool: ProtectToolId | null) {
    if (tool === this.tool) return;
    this.pageTool?.finish?.();
    if (tool) this.#host.takePageTool();
    this.tool = tool;
    if (tool) this.deselectMark();
  }

  /** another tool took over the pages. */
  dropTool() {
    if (this.tool) this.setTool(null);
  }

  /**
   * escape: lets go of the selected mark, then the tool. false when there
   * was nothing to let go of, so the next thing in line can have it.
   */
  escape(): boolean {
    if (this.selectedMarkId) {
      this.deselectMark();
      return true;
    }
    if (this.tool) {
      this.setTool(null);
      return true;
    }
    return false;
  }

  // ---- marking ----

  #add(added: NewMark[], select: boolean): PendingMark[] {
    const result = addMarks(this.marks, added, this.#newId);
    this.marks = result.marks;
    for (const page of markedPages(result.added)) this.#remember(page);
    if (result.added.length > 0) {
      this.#host.announce(`${areaCount(result.added.length)} marked on ${this.#pagesText(markedPages(result.added))}.`);
      if (select && result.added.length === 1) this.selectMark(result.added[0].id, true);
    }
    return result.added;
  }

  #pagesText(pages: number[]): string {
    return pages.length === 1 ? `page ${pages[0] + 1}` : `pages ${pages.map((page) => page + 1).join(", ")}`;
  }

  /** marks selected text: one area for each line on each page. */
  markSelection(selection: TextSelection | null = this.#host.text.selection()): boolean {
    if (!selection || isCollapsed(selection)) return false;
    const words = selectedText(selection, (page) => this.#host.text.text(page));
    const added: NewMark[] = [];
    for (const page of selectedPages(selection)) {
      const text = this.#host.text.text(page);
      const size = this.#host.pageSize(page);
      if (!text || !size) continue;
      for (const rect of selectionRects(selection, page, text)) {
        added.push({ page, rect: padTextBox(rect, size), source: "text", label: words });
      }
    }
    this.#host.text.setSelection(null);
    return this.#add(added, false).length > 0;
  }

  markArea(page: number, rect: Rect) {
    this.#add([{ page, rect, source: "area", label: "" }], true);
  }

  /** a box in the middle of the page on screen, selected so the arrows can move it. */
  markAreaFromKeyboard() {
    const centre = this.#host.viewCentre();
    const size = this.#host.pageSize(centre.page);
    if (!size) return;
    this.markArea(centre.page, rectAround(centre, KEYBOARD_AREA.width, KEYBOARD_AREA.height, size));
  }

  markFound(items: FoundItem[]): number {
    return this.#add(marksFromFound(items), false).length;
  }

  removeMark(id: string) {
    const mark = this.marks.find((each) => each.id === id);
    this.marks = removeMark(this.marks, id);
    if (this.selectedMarkId === id) this.deselectMark();
    if (mark) this.#host.announce(`Mark removed from page ${mark.page + 1}.`);
    // an open review describes the marks as they were: look again
    if (this.review) {
      this.closeReview();
      if (this.marks.length > 0) this.openReview();
    }
  }

  moveSelectedMark(rect: Rect) {
    if (!this.selectedMarkId) return;
    this.marks = moveMark(this.marks, this.selectedMarkId, rect);
    this.framePreview = null;
  }

  clearMarks() {
    this.marks = [];
    this.deselectMark();
  }

  /** selects a mark; `focus` gives its frame the keyboard. */
  selectMark(id: string, focus = false) {
    this.selectedMarkId = id;
    this.framePreview = null;
    if (focus) this.focusTick += 1;
  }

  /** goes to a mark on its page and gives its frame the keyboard. */
  showMark(id: string) {
    const mark = this.marks.find((each) => each.id === id);
    if (!mark) return;
    this.#host.reveal(mark.page, mark.rect);
    this.selectMark(id, true);
  }

  deselectMark() {
    this.selectedMarkId = null;
    this.framePreview = null;
  }

  // ---- marks and other edits ----

  async #signatureOf(page: number): Promise<string> {
    const size = this.#host.pageSize(page);
    const known = this.#host.text.text(page);
    const text = known ?? (await fetchPageText(this.#host.documentId(), page).catch(() => undefined));
    return pageSignature(size, text ? text.chars.map((char) => char.ch).join("") : "");
  }

  async #remember(page: number) {
    if (!this.#signatures.has(page)) this.#signatures.set(page, await this.#signatureOf(page));
  }

  /**
   * after every change to the document: the status is read again, and
   * marks on pages another tool changed (turned, cropped, moved, deleted)
   * are dropped, with a word about it, instead of landing on the wrong words.
   */
  async setRevision(revision: number) {
    if (revision === this.#revision) return;
    this.#revision = revision;
    this.refreshStatus();
    if (this.marks.length === 0) return;
    const now = new Map<number, string>();
    for (const page of markedPages(this.marks)) {
      // the page text the viewer had is from the version before
      const size = this.#host.pageSize(page);
      const text = await fetchPageText(this.#host.documentId(), page).catch(() => undefined);
      now.set(page, pageSignature(size, text ? text.chars.map((char) => char.ch).join("") : ""));
    }
    if (revision !== this.#revision) return;
    const { kept, droppedPages } = keepMatchingPages(this.marks, this.#signatures, now);
    if (droppedPages.length === 0) return;
    this.marks = kept;
    for (const page of droppedPages) this.#signatures.delete(page);
    if (this.selectedMark === null) this.deselectMark();
    this.#host.notify(`Marks on ${this.#pagesText(droppedPages)} were removed because the page changed. Mark it again.`);
  }

  refreshStatus() {
    protectionStatus(this.#host.documentId())
      .then((status) => (this.status = status))
      .catch(() => (this.status = null));
  }

  // ---- review and apply ----

  async openReview() {
    if (this.marks.length === 0) return;
    const id = this.#host.documentId();
    const areas = asRedactionAreas(this.marks);
    const first = Math.min(...this.marks.map((mark) => mark.page));
    this.review = { loading: true, preview: null, dryRun: null, pages: [], warnings: [], shownPage: first, error: null };
    try {
      const [preview, dryRun] = await Promise.all([redactionPreview(id, areas), redactionDryRun(id, areas, this.options)]);
      if (!this.review) {
        closeRedactionDryRun(id, dryRun.previewId);
        return;
      }
      const pages = reviewPages(this.marks, preview, dryRun.report);
      const removesText = preview.areas.some((area) => area.texts.length > 0);
      this.review = { ...this.review, loading: false, preview, dryRun, pages, warnings: reviewWarnings(dryRun.report, removesText) };
    } catch (problem) {
      const message = typeof problem === "object" && problem && "message" in problem ? String(problem.message) : String(problem);
      if (this.review) this.review = { ...this.review, loading: false, error: message };
    }
  }

  closeReview() {
    const dryRun = this.review?.dryRun;
    this.review = null;
    if (dryRun) closeRedactionDryRun(this.#host.documentId(), dryRun.previewId).catch(() => {});
  }

  /** Find and Mark, opened from a warning about words left elsewhere. */
  findElsewhere(phrase: string) {
    this.closeReview();
    this.findRequest = { phrase, at: performance.now() };
    this.#host.openTool("redactFind");
  }

  /** opens the inspector at another tool's section. */
  openTool(tool: ToolName) {
    this.#host.openTool(tool);
  }

  askToApply() {
    if (this.marks.length > 0) this.confirmingApply = true;
  }

  /** applies every mark: one change, one undo step. */
  async apply(): Promise<boolean> {
    this.confirmingApply = false;
    const marks = this.marks;
    if (marks.length === 0) return false;
    this.closeReview();
    const id = this.#host.documentId();
    const areas = asRedactionAreas(marks);
    const options = { ...this.options, overlayText: this.options.overlayText?.trim() || null };
    const worked = await this.#host.change(async () => (await applyRedactions(id, areas, options)).document);
    if (!worked) return false;
    this.clearMarks();
    this.#signatures.clear();
    this.setTool(null);
    this.lastApplied = marks.length;
    const message = appliedMessage(marks.length);
    this.#host.announce(message);
    this.#host.notify(message, { label: "Remove Hidden Information…", run: () => this.#host.openTool("removeHiddenInfo") });
    this.refreshStatus();
    return true;
  }

  // ---- passwords ----

  async unlock(password: string): Promise<boolean> {
    const id = this.#host.documentId();
    const worked = await this.#host.change(() => unlockDocument(id, password));
    if (worked) this.#host.announce("Unlocked for editing.");
    this.refreshStatus();
    return worked;
  }

  async changeLock(change: LockChange, announcement: string): Promise<boolean> {
    const id = this.#host.documentId();
    const worked = await this.#host.change(() => changeLockOnSave(id, change));
    if (worked) this.#host.announce(announcement);
    this.refreshStatus();
    return worked;
  }

  // ---- the window ----

  /** whether the document may close: asks first when marks would be lost. */
  mayLeave(): Promise<boolean> {
    const count = this.marks.length;
    if (count === 0) return Promise.resolve(true);
    return new Promise((resolve) => {
      this.leaveQuestion = {
        count,
        answer: (discard) => {
          this.leaveQuestion = null;
          if (discard) this.clearMarks();
          resolve(discard);
        },
      };
    });
  }

  /** what each Protect command does in this window. */
  #handlers(): Record<ProtectCommand, () => void> {
    const open = (tool: ToolName) => this.#host.openTool(tool);
    return {
      "mark-redaction": () => {
        if (this.markSelection()) return;
        open("redactMarkText");
        this.setTool("markText");
      },
      "mark-redaction-area": () => {
        open("redactMarkArea");
        // the key a second time places a box, for the keyboard
        if (this.tool === "markArea") this.markAreaFromKeyboard();
        else this.setTool("markArea");
      },
      "find-and-mark": () => open("redactFind"),
      "review-redactions": () => {
        open("redactReview");
        this.openReview();
      },
      "apply-redactions": () => {
        open("redactApply");
        this.askToApply();
      },
      "remove-hidden-information": () => open("removeHiddenInfo"),
      "password-settings": () => open(this.status?.locked ? "unlockForEditing" : "openPassword"),
    };
  }

  /** registers the commands and the close question while the viewer is shown; returns the undo. */
  attach(dispatcher: Dispatcher): () => void {
    const handlers = this.#handlers();
    const takeBack = PROTECT_COMMANDS.map((command) => dispatcher.register(command, handlers[command]));
    protectGuard.mayLeave = () => this.mayLeave();
    return () => {
      takeBack.forEach((unregister) => unregister());
      if (protectGuard.mayLeave) protectGuard.mayLeave = null;
      this.closeReview();
    };
  }
}

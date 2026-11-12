// one open presentation while its window shows it: the deck's summary,
// the slide on screen, the selection, each slide's objects and picture
// (fetched when first needed and again after an edit changed them), the
// undo state, and saving. every change goes to the shell as one of the
// engine's operations; the shell answers with what to redraw.

import { asCommandError } from "../api";
import type { Operation } from "./operations";
import {
  applyOperation,
  autosaveDeck,
  chooseDeckSaveLocation,
  discardDeckRecovery,
  insertObject,
  insertPicture,
  redoEdit,
  replaceOne,
  saveDeck,
  setSlideHidden,
  duplicateObjects,
  slidePictures,
  slideView,
  undoEdit,
  type DeckFormat,
  type DeckView,
  type Edited,
  type Found,
  type HistoryState,
  type NewObject,
  type OpenedDeck,
  type PointBox,
  type SlidePicture,
  type SlideView,
} from "./slidesApi";

/** recovery copies are written this often while there are unsaved changes, as the writer does */
export const AUTOSAVE_INTERVAL_MS = 5000;

/** thumbnails are fetched in small batches so the first ones show at once */
const PICTURE_BATCH = 6;

export interface Notice {
  text: string;
  action?: { label: string; run: () => void };
}

export class DeckSession {
  readonly id: number;
  name = $state("");
  path = $state("");
  format = $state<DeckFormat>("pptx");
  deck = $state<DeckView>({ width: 960, height: 540, slides: [], layouts: [], theme: null });
  history = $state<HistoryState>({ changedSlides: [], allSlides: false, canUndo: false, canRedo: false, dirty: false });
  current = $state(0);
  /** selected objects on the current slide, in the order they were picked */
  selection = $state<number[]>([]);
  /** selected slides in the rail (the current one is always among them) */
  selectedSlides = $state<string[]>([]);
  views = $state<Record<string, SlideView>>({});
  pictures = $state<Record<string, SlidePicture>>({});
  notice = $state<Notice | null>(null);
  /** the table cell last typed in, for Rows and Columns in the inspector */
  tableCell = $state<{ object: number; row: number; column: number } | null>(null);
  /** set while an edit is on its way, so a second one waits */
  private queue: Promise<unknown> = Promise.resolve();
  /** edits so far, and how many the last recovery copy had */
  private edits = 0;
  private autosavedEdits = 0;

  constructor(
    opened: OpenedDeck,
    private readonly onError: (message: string) => void,
  ) {
    this.id = opened.id;
    this.name = opened.name;
    this.path = opened.path;
    this.format = opened.format;
    this.deck = opened.deck;
    this.history = opened.state;
    this.selectedSlides = opened.deck.slides.length > 0 ? [opened.deck.slides[0].id] : [];
  }

  get slide() {
    return this.deck.slides[this.current] ?? null;
  }

  get view(): SlideView | null {
    const slide = this.slide;
    return slide ? (this.views[slide.id] ?? null) : null;
  }

  /** shows a slide: its objects and picture are fetched if they aren't here. */
  async go(index: number, keepRailSelection = false) {
    const clamped = Math.max(0, Math.min(index, this.deck.slides.length - 1));
    if (clamped !== this.current) this.selection = [];
    this.current = clamped;
    const slide = this.slide;
    if (!slide) return;
    if (!keepRailSelection) this.selectedSlides = [slide.id];
    await Promise.all([this.loadView(slide.id), this.loadPictures([slide.id])]);
  }

  async loadView(slideId: string) {
    if (this.views[slideId]) return;
    const view = await slideView(this.id, slideId).catch((problem) => this.fail(problem));
    if (view) this.views = { ...this.views, [slideId]: view };
  }

  /** the pictures of these slides, in batches, skipping those already here. */
  async loadPictures(slideIds: string[]) {
    const missing = slideIds.filter((id) => !this.pictures[id]);
    for (let start = 0; start < missing.length; start += PICTURE_BATCH) {
      const batch = missing.slice(start, start + PICTURE_BATCH);
      const drawn = await slidePictures(this.id, batch).catch((problem) => this.fail(problem));
      if (!drawn) return;
      const next = { ...this.pictures };
      for (const picture of drawn) next[picture.slideId] = picture;
      this.pictures = next;
    }
  }

  /** runs one operation; `select` picks objects afterwards. */
  apply(operation: Operation, select?: number[]): Promise<boolean> {
    return this.run(() => applyOperation(this.id, operation), select);
  }

  /** Duplicate: copies of the selected objects, the last one selected. */
  duplicate() {
    const slide = this.slide;
    if (!slide || this.selection.length === 0) return Promise.resolve(false);
    const ids = [...this.selection];
    return this.run(() => duplicateObjects(this.id, slide.id, ids));
  }

  /** Replace: one found place's words, as one undo step. */
  replace(found: Found, words: string, replacement: string, matchCase: boolean) {
    return this.run(() => replaceOne(this.id, found, words, replacement, matchCase));
  }

  /** Hide Slide or Show Slide for a slide. */
  setHidden(slideId: string, hidden: boolean) {
    return this.run(() => setSlideHidden(this.id, slideId, hidden));
  }

  undo() {
    return this.run(() => undoEdit(this.id));
  }

  redo() {
    return this.run(() => redoEdit(this.id));
  }

  /** adds a shape, text box or table to the current slide and selects it. */
  insert(object: NewObject, at: PointBox) {
    const slide = this.slide;
    if (!slide) return Promise.resolve(false);
    return this.run(() => insertObject(this.id, slide.id, object, at));
  }

  insertPicture(path: string, at: PointBox) {
    const slide = this.slide;
    if (!slide) return Promise.resolve(false);
    return this.run(() => insertPicture(this.id, slide.id, path, at));
  }

  /** edits wait for each other, so two quick presses never cross. */
  private run(edit: () => Promise<Edited>, select?: number[]): Promise<boolean> {
    const next = this.queue.then(async () => {
      try {
        await this.accept(await edit(), select);
        return true;
      } catch (problem) {
        this.fail(problem);
        return false;
      }
    });
    this.queue = next;
    return next;
  }

  /** takes an edit's answer: forgets what changed and fetches the slide on screen again. */
  private async accept(edited: Edited, select?: number[]) {
    const currentId = this.slide?.id;
    this.edits += 1;
    this.deck = edited.deck;
    this.history = edited.change;
    const changed = edited.change.allSlides ? Object.keys(this.views) : edited.change.changedSlides;
    const stillThere = this.deck.slides.findIndex((slide) => slide.id === currentId);
    this.current = stillThere >= 0 ? stillThere : Math.max(0, Math.min(this.current, this.deck.slides.length - 1));
    const redrawn = this.forget(changed, edited.change.allSlides);
    if (edited.added !== null) this.selection = [edited.added];
    else if (select) this.selection = select;
    const shown = this.slide;
    const reloaded = shown && changed.includes(shown.id) ? this.reloadView(shown.id) : Promise.resolve();
    // the edit counts as done once the slide on screen has its new objects and picture
    await Promise.all([redrawn, reloaded, this.go(this.current, true)]);
  }

  /** the slide's objects fetched again; the old ones stay on screen until they come. */
  private async reloadView(slideId: string) {
    const view = await slideView(this.id, slideId).catch((problem) => this.fail(problem));
    if (!view) return;
    this.views = { ...this.views, [slideId]: view };
    // an undo can take a selected object away; the selection keeps only what is still there
    if (slideId === this.slide?.id) this.selection = this.selection.filter((id) => view.objects.some((object) => object.id === id));
  }

  /** drops changed slides' objects (but the one on screen) and redraws their pictures. */
  private forget(slideIds: string[], all: boolean): Promise<void> {
    const views = { ...this.views };
    const shown = this.slide?.id;
    for (const id of slideIds) if (id !== shown) delete views[id];
    this.views = views;
    const ids = all ? this.deck.slides.map((slide) => slide.id) : slideIds;
    // the old picture keeps showing while the new one is drawn, so nothing flickers
    return slidePictures(this.id, ids.filter((id) => this.deck.slides.some((slide) => slide.id === id)))
      .then((drawn) => {
        const next = { ...this.pictures };
        for (const picture of drawn) next[picture.slideId] = picture;
        this.pictures = next;
      })
      .catch((problem) => this.fail(problem));
  }

  // --- saving

  async save(): Promise<boolean> {
    return this.writeTo(null);
  }

  async saveAs(): Promise<boolean> {
    const chosen = await chooseDeckSaveLocation(this.path, this.format);
    return chosen ? this.writeTo(chosen) : false;
  }

  private async writeTo(path: string | null): Promise<boolean> {
    await this.queue;
    try {
      const saved = await saveDeck(this.id, path);
      this.name = saved.name;
      this.path = saved.path;
      this.format = saved.format;
      this.history = saved.state;
      if (saved.notes.length > 0) this.notice = { text: `Saved as OpenDocument. ${saved.notes.join(" ")}` };
      return true;
    } catch (problem) {
      this.fail(problem);
      return false;
    }
  }

  /** a recovery copy, once per change. */
  async autosave() {
    if (!this.history.dirty || this.edits === this.autosavedEdits) return;
    this.autosavedEdits = this.edits;
    await autosaveDeck(this.id).catch(() => {});
  }

  async discardRecovery() {
    await discardDeckRecovery(this.id).catch(() => {});
  }

  fail(problem: unknown): undefined {
    this.onError(asCommandError(problem).message);
    return undefined;
  }
}

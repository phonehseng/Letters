// one open writer document: the editor, its state as the screen sees it,
// and every command the toolbar, menus, keys and inspector use.
//
// the editor (prosemirror) owns the document and its undo history. this
// class keeps a copy of the editor state in a svelte state field, so
// everything on screen follows each change, and it runs the shell calls
// (save, autosave, export, pictures).

import { NodeSelection, TextSelection, type EditorState, type Transaction } from "prosemirror-state";
import { addColumnAfter, addColumnBefore, addRowAfter, addRowBefore, deleteColumn, deleteRow, isInTable, mergeCells, splitCell } from "prosemirror-tables";
import { redo as historyRedo, undo as historyUndo, undoDepth, redoDepth } from "prosemirror-history";
import { EditorState as PmEditorState } from "prosemirror-state";
import { EditorView } from "prosemirror-view";
import { modelToDoc, docToModel } from "./convert";
import { countDocument, countRange } from "./counts";
import * as edit from "./editing";
import * as tableEdits from "./tableEdits";
import * as pictures from "./pictureCommands";
import { findAll, replaceAll, replaceMatch, type FindOptions, type Match } from "./find";
import { imageView, MediaCache } from "./images";
import type { Block, DocumentMeta, DocumentModel, TabStop } from "./model";
import { buildOutline, moveSection, shiftedLevel, type OutlineEntry } from "./outline";
import { findKey, pagesKey, writerPlugins, type PageGap } from "./plugins";
import { schema } from "./schema";
import { headingStyle } from "./styles";
import { acceptChange, listChanges, nextChangeId, rejectChange, resolveAll, trackTransaction, type TrackedChange } from "./tracked";
import { defaultTypingSettings, toggleForFeature, type Replacement } from "./typing";
import type { WriterCommand } from "./writerCommands";
import * as api from "./writerApi";
import { WriterObjects } from "./writerObjects.svelte";
import { copySelection, pasteFromClipboard } from "./clipboardBridge";
import { saveFormatsFrom, type SaveFormat } from "./formats";
import { applyEngineModel } from "./textBlocks";

type Data = Record<string, unknown>;

export interface WriterNotice {
  text: string;
  action?: { label: string; run: () => void };
}

export class WriterSession {
  id: number;
  name = $state("");
  path = $state("");
  /** the file changed on disk since this window read it: the sentence while Replace, Cancel or Save As is asked */
  changedOnDisk = $state<string | null>(null);
  kind = $state<"docx" | "odt">("docx");
  /** the editor state as of the last change; read it, never change it */
  editor = $state.raw<EditorState>(PmEditorState.create({ schema }));
  saved = $state.raw<EditorState["doc"] | null>(null);
  settings = $state<api.WriterSettings>({ typing: defaultTypingSettings(), spellcheck: true });
  tracking = $state(false);
  notice = $state<WriterNotice | null>(null);
  announcement = $state("");
  author = "";
  view: EditorView | null = null;
  media: MediaCache;
  /** text blocks, free pictures, stacking and aligning (writerObjects.svelte.ts) */
  objects: WriterObjects;
  /**
   * a document converted from another file (a pdf, rtf or .doc) has never
   * been saved: it counts as changed, and its first Save asks where to put it
   */
  neverSaved = $state(false);
  private changeIds = 1000;
  private autosaved: EditorState["doc"] | null = null;
  private onError: (message: string) => void;

  constructor(opened: api.WriterOpened, onError: (message: string) => void) {
    this.id = opened.id;
    this.name = opened.name;
    this.path = opened.path;
    this.kind = opened.kind;
    this.onError = onError;
    this.media = new MediaCache((mediaId) => api.mediaUrl(this.id, mediaId));
    this.objects = new WriterObjects(this);
    this.editor = PmEditorState.create({ doc: modelToDoc(opened.model), plugins: this.plugins() });
    this.saved = this.editor.doc;
    this.autosaved = this.editor.doc;
    this.neverSaved = !!opened.convertedFrom;
    this.tracking = !!opened.model.settings?.trackChanges;
    this.changeIds = nextChangeId(this.editor.doc);
    api.writerSettings().then((settings) => (this.settings = settings)).catch(() => {});
    this.checkCompatibility();
    api.authorName().then((name) => (this.author = name || "Letters"));
  }

  private plugins() {
    return writerPlugins({
      typing: () => this.settings.typing,
      onAutocorrect: (change) => this.afterAutocorrect(change),
      onPreservedRefused: () => this.say("That text holds an item Letters keeps as it is. To delete the item, click it and press Delete."),
    });
  }

  // --- the editor view

  /** puts the editor into `host`; called once by the page surface. */
  mount(host: HTMLElement) {
    this.view = new EditorView(host, {
      state: this.editor,
      nodeViews: { image: imageView(this.media) },
      attributes: { role: "textbox", "aria-multiline": "true", "aria-label": `${this.name}, document text`, class: "w-doc w-body" },
      dispatchTransaction: (tr) => this.dispatch(tr),
      // pictures and html with pictures pasted from other apps go through
      // the engine (clipboardBridge.ts); copying a picture carries its bytes
      handlePaste: (_view, event) => pasteFromClipboard(this, event),
      handleDOMEvents: {
        copy: (_view, event) => copySelection(this, event as ClipboardEvent, false),
        cut: (_view, event) => copySelection(this, event as ClipboardEvent, true),
      },
      handleDrop: (view, event) => {
        const files = [...(event.dataTransfer?.files ?? [])].filter((file) => file.type.startsWith("image/"));
        if (files.length === 0) return false;
        const at = view.posAtCoords({ left: event.clientX, top: event.clientY });
        this.insertPictureFiles(files, at?.pos);
        return true;
      },
      handleClickOn: (_view, _pos, node, nodePos, event) => {
        // links open with ctrl or command held, as in word
        const link = (event.target as HTMLElement).closest("a.w-link") as HTMLAnchorElement | null;
        if (link && (event.ctrlKey || event.metaKey)) {
          window.open(link.href, "_blank");
          return true;
        }
        void node;
        void nodePos;
        return false;
      },
    });
    return () => {
      this.view?.destroy();
      this.view = null;
    };
  }

  dispatch(tr: Transaction) {
    if (!this.view) return;
    const tracked = this.tracking ? trackTransaction(this.view.state, tr, this.tracker()) : null;
    const next = this.view.state.apply(tracked ?? tr);
    this.view.updateState(next);
    this.editor = next;
  }

  private tracker() {
    return { author: this.author || "Letters", date: new Date().toISOString().replace(/\.\d+Z$/, "Z"), nextId: () => String((this.changeIds += 1)) };
  }

  /** runs a command made from the current state; returns false when it did not apply. */
  run(make: (state: EditorState) => Transaction | null, focus = true): boolean {
    const tr = make(this.view?.state ?? this.editor);
    if (!tr) return false;
    this.dispatch(tr.scrollIntoView());
    if (focus) this.view?.focus();
    return true;
  }

  get dirty(): boolean {
    if (this.neverSaved) return true;
    return !!this.saved && !this.editor.doc.eq(this.saved);
  }

  /** the file moved (organising in Recents or a folder): Save writes to the new place. */
  async followMove(to: string) {
    try {
      const followed = await api.followMove(this.id, to);
      this.path = followed.path;
      this.name = followed.name;
    } catch (problem) {
      this.onError(errorText(problem));
    }
  }

  get meta(): DocumentMeta {
    return this.editor.doc.attrs.meta as DocumentMeta;
  }

  get canUndo(): boolean {
    return undoDepth(this.editor) > 0;
  }

  get canRedo(): boolean {
    return redoDepth(this.editor) > 0;
  }

  model(): DocumentModel {
    return docToModel(this.view?.state.doc ?? this.editor.doc);
  }

  // --- messages

  /** a problem from the shell, said in its own words. */
  fail(problem: unknown) {
    this.onError(errorText(problem));
  }

  say(text: string, action?: WriterNotice["action"]) {
    this.notice = { text, action };
  }

  /** a polite announcement for screen readers. */
  announce(text: string) {
    this.announcement = "";
    queueMicrotask(() => (this.announcement = text));
  }

  private afterAutocorrect(change: Replacement) {
    const toggle = toggleForFeature(change.feature);
    if (toggle && change.feature !== "smartQuotes" && change.feature !== "capitalizeSentences") {
      this.announce(`Corrected to “${change.insert.trim()}”. Press Backspace to undo.`);
    }
  }

  // --- saving

  async save(path: string | null = null, replaceChanged = false): Promise<boolean> {
    const model = this.model();
    const doc = this.view?.state.doc ?? this.editor.doc;
    try {
      const saved = await api.saveWriterDocument(this.id, model, path, replaceChanged);
      this.name = saved.name;
      this.path = saved.path;
      this.kind = saved.kind;
      this.saved = doc;
      this.autosaved = doc;
      this.neverSaved = false;
      this.announce(`Saved “${saved.name}”.`);
      return true;
    } catch (problem) {
      // another window or program changed the file: the screen asks (ChangedOnDiskSheet)
      if (errorKind(problem) === "changed-on-disk") this.changedOnDisk = errorText(problem);
      else this.onError(errorText(problem));
      return false;
    }
  }

  /** writes the recovery copy if there are changes it does not have yet. */
  async autosave() {
    const doc = this.editor.doc;
    if (!this.dirty || (this.autosaved && doc.eq(this.autosaved))) return;
    this.autosaved = doc;
    try {
      await api.autosaveWriterDocument(this.id, this.model());
    } catch {
      // a recovery copy that could not be written is tried again next time
      this.autosaved = null;
    }
  }

  discardRecovery() {
    return api.discardRecovery(this.id).catch(() => {});
  }

  // --- undo and redo

  undo() {
    if (this.view) historyUndo(this.view.state, (tr) => this.dispatch(tr));
  }

  redo() {
    if (this.view) historyRedo(this.view.state, (tr) => this.dispatch(tr));
  }

  // --- commands by name (menus, keys, toolbar)

  command(name: WriterCommand): boolean {
    switch (name) {
      case "format-bold":
        return this.run((s) => edit.toggleMark(s, "bold"));
      case "format-italic":
        return this.run((s) => edit.toggleMark(s, "italic"));
      case "format-underline":
        return this.run((s) => edit.toggleMark(s, "underline", { style: "single" }));
      case "format-strike":
        return this.run((s) => edit.toggleMark(s, "strike"));
      case "format-superscript":
        return this.run((s) => edit.toggleMark(s, "superscript"));
      case "format-subscript":
        return this.run((s) => edit.toggleMark(s, "subscript"));
      case "style-title":
        return this.applyStyle("title");
      case "style-heading-1":
        return this.applyStyle("h1");
      case "style-heading-2":
        return this.applyStyle("h2");
      case "style-heading-3":
        return this.applyStyle("h3");
      case "style-body":
        return this.applyStyle("body");
      case "style-quote":
        return this.applyStyle("quote");
      case "align-left":
        return this.run((s) => edit.setAlignment(s, "left"));
      case "align-center":
        return this.run((s) => edit.setAlignment(s, "center"));
      case "align-right":
        return this.run((s) => edit.setAlignment(s, "right"));
      case "align-justify":
        return this.run((s) => edit.setAlignment(s, "justify"));
      case "list-bullets":
        return this.run((s) => edit.toggleList(s, "bullet"));
      case "list-numbers":
        return this.run((s) => edit.toggleList(s, "number"));
      case "indent-more":
        return this.run((s) => edit.changeListLevel(s, "in") ?? edit.setIndent(s, (indent) => ({ ...indent, left: (indent.left ?? 0) + 720 })));
      case "indent-less":
        return this.run((s) => edit.changeListLevel(s, "out") ?? edit.setIndent(s, (indent) => ({ ...indent, left: Math.max(0, (indent.left ?? 0) - 720) })));
      case "insert-page-break":
        return this.run((s) => edit.insertPageBreak(s));
      case "insert-footnote":
        return this.insertFootnote();
      case "insert-page-number":
        return this.run((s) => edit.insertPageNumber(s));
      case "track-changes":
        this.setTracking(!this.tracking);
        return true;
      default:
        // the rest open something on screen; the writer screen handles them
        return false;
    }
  }

  applyStyle(key: string): boolean {
    const done = this.run((s) => edit.applyStyle(s, key));
    if (done) {
      const label = key === "body" ? "Body" : key === "title" ? "Title" : key === "quote" ? "Quote" : /^h\d$/.test(key) ? `Heading ${key.slice(1)}` : "the style";
      this.announce(`${label} applied.`);
    }
    return done;
  }

  setTracking(on: boolean) {
    this.tracking = on;
    // the setting is saved with the document, as word does
    const meta = this.meta;
    this.run((s) => edit.setMeta(s.tr, { ...meta, settings: { ...(meta.settings ?? {}), trackChanges: on } }).setMeta("untracked", true), false);
    this.announce(on ? "Track Changes is on. Your edits are recorded." : "Track Changes is off.");
  }

  // --- inserting

  insertTable(rows: number, columns: number) {
    this.run((s) => edit.insertTable(s, rows, columns));
    this.announce(`Table with ${rows} rows and ${columns} columns added.`);
  }

  /** Insert › Picture… (pictureCommands.ts) */
  insertPicture() {
    return pictures.insertPicture(this);
  }

  insertFootnote(): boolean {
    const view = this.view;
    if (!view) return false;
    const { tr, noteId } = edit.insertFootnote(view.state);
    this.dispatch(tr.scrollIntoView());
    this.focusNote = noteId;
    this.announce("Footnote added. Type the note under the text.");
    return true;
  }

  /** a footnote whose editor should take the focus once it is shown */
  focusNote = $state<string | null>(null);

  setLink(href: string | null) {
    this.run((s) => edit.setLink(s, href));
  }

  addComment(text: string): boolean {
    const view = this.view;
    if (!view) return false;
    const added = edit.addComment(view.state, this.author || "Letters", text, new Date().toISOString().replace(/\.\d+Z$/, "Z"));
    if (!added) return false;
    this.dispatch(added.tr);
    this.announce("Comment added.");
    return true;
  }

  // --- tables (prosemirror-tables commands)

  table(action: "rowBefore" | "rowAfter" | "deleteRow" | "columnBefore" | "columnAfter" | "deleteColumn" | "merge" | "split") {
    const commands = { rowBefore: addRowBefore, rowAfter: addRowAfter, deleteRow, columnBefore: addColumnBefore, columnAfter: addColumnAfter, deleteColumn, merge: mergeCells, split: splitCell };
    if (!this.view) return;
    commands[action](this.view.state, (tr) => this.dispatch(tr));
    this.view.focus();
  }

  inTable(): boolean {
    return isInTable(this.editor);
  }

  /** changes the table around the cursor (tableEdits.ts). */
  updateTable(change: (data: Data) => Data) {
    this.run((state) => tableEdits.updateTable(state, change));
  }

  /** changes each selected cell, or the cell with the cursor. */
  updateCells(change: (data: Data) => Data) {
    this.run((state) => tableEdits.updateCells(state, change));
  }

  setHeaderRow(on: boolean) {
    this.run((state) => tableEdits.setHeaderRow(state, on));
  }

  // --- the outline

  outline(): OutlineEntry[] {
    return buildOutline(this.editor.doc);
  }

  goToHeading(entry: OutlineEntry) {
    if (!this.view) return;
    const tr = this.view.state.tr.setSelection(TextSelection.create(this.view.state.doc, entry.pos + 1));
    this.dispatch(tr.scrollIntoView());
    this.view.focus();
  }

  shiftHeading(entry: OutlineEntry, direction: "promote" | "demote") {
    const level = shiftedLevel(entry.level, direction);
    if (level === null) return;
    this.run((state) => {
      const node = state.doc.child(entry.index);
      const { id, sheet } = headingStyle((state.doc.attrs.meta as DocumentMeta).styles, level);
      const tr = state.tr;
      if (sheet !== (state.doc.attrs.meta as DocumentMeta).styles) edit.setMeta(tr, { ...(state.doc.attrs.meta as DocumentMeta), styles: sheet });
      return tr.setNodeMarkup(entry.pos, undefined, { level, data: { ...(node.attrs.data as Data), styleId: id } });
    }, false);
    this.announce(`${entry.text || "Heading"} is now Heading ${level}.`);
  }

  moveSection(entry: OutlineEntry, beforeIndex: number) {
    this.run((state) => {
      const tr = state.tr.setMeta("untracked", true);
      return moveSection(tr, entry.index, beforeIndex) ? tr : null;
    }, false);
  }

  // --- find and replace

  find(query: string, options: FindOptions, current: number): Match[] {
    const matches = findAll(this.editor.doc, query, options);
    this.view?.dispatch(this.view.state.tr.setMeta(findKey, { matches, current }).setMeta("addToHistory", false));
    return matches;
  }

  showMatch(matches: Match[], index: number) {
    if (!this.view || !matches[index]) return;
    const match = matches[index];
    const tr = this.view.state.tr.setSelection(TextSelection.create(this.view.state.doc, match.from, match.to)).setMeta(findKey, { matches, current: index });
    this.view.dispatch(tr.scrollIntoView());
  }

  clearFind() {
    this.view?.dispatch(this.view.state.tr.setMeta(findKey, { matches: [], current: -1 }));
  }

  replace(match: Match, replacement: string) {
    this.run((state) => {
      const tr = state.tr;
      replaceMatch(tr, match, replacement);
      return tr;
    }, false);
  }

  replaceAll(query: string, replacement: string, options: FindOptions): number {
    let count = 0;
    this.run((state) => {
      const tr = state.tr;
      count = replaceAll(tr, query, replacement, options);
      return count ? tr : null;
    }, false);
    this.announce(count === 1 ? "Replaced 1 match." : `Replaced ${count} matches.`);
    return count;
  }

  // --- tracked changes

  changes(): TrackedChange[] {
    return listChanges(this.editor.doc);
  }

  resolveChange(change: TrackedChange, accept: boolean) {
    this.run((state) => {
      const tr = state.tr.setMeta("untracked", true);
      if (accept) acceptChange(tr, change);
      else rejectChange(tr, change);
      return tr;
    }, false);
    this.announce(accept ? "Change accepted." : "Change rejected.");
  }

  resolveAllChanges(accept: boolean) {
    this.run((state) => {
      const tr = state.tr.setMeta("untracked", true);
      return resolveAll(tr, accept) ? tr : null;
    }, false);
    this.announce(accept ? "All changes accepted." : "All changes rejected.");
  }

  goTo(pos: number, to = pos) {
    if (!this.view) return;
    const doc = this.view.state.doc;
    const selection = to > pos ? TextSelection.create(doc, pos, to) : TextSelection.near(doc.resolve(pos));
    this.dispatch(this.view.state.tr.setSelection(selection).scrollIntoView());
    this.view.focus();
  }

  // --- headers, footers and notes (their small editors write back here)

  setNoteBlocks(noteId: string, blocks: Block[]) {
    this.run((s) => edit.setNoteBlocks(s, noteId, blocks), false);
  }

  setHeaderFooterBlocks(id: string, blocks: Block[]) {
    this.run((s) => edit.setHeaderFooterBlocks(s, id, blocks), false);
  }

  // --- pages

  setPageGaps(gaps: PageGap[]) {
    this.view?.dispatch(this.view.state.tr.setMeta(pagesKey, gaps).setMeta("addToHistory", false));
  }

  // --- presets (the engine applies them; the result is one undo step)

  async applyPreset(preset: "mla" | "apa", details: Record<string, unknown>) {
    try {
      const next = await api.applyPreset(this.model(), preset, details);
      this.run((s) => edit.replaceDocument(s, modelToDoc(next)).setMeta("untracked", true));
      this.say(`${preset.toUpperCase()} applied to the whole document. Undo takes it back.`);
    } catch (problem) {
      this.onError(errorText(problem));
    }
  }

  // --- export

  async exportPdf(path: string): Promise<void> {
    this.say("Exporting… Word or LibreOffice can take a few seconds.");
    try {
      const answer = await api.exportPdf(this.id, this.model(), path);
      if (answer.exported) {
        const exported = answer.exported;
        this.say(`Exported with ${exported.methodLabel}: “${exported.path.split(/[\\/]/).pop()}”.`, {
          label: "Open PDF in Letters",
          run: () => import("../api").then(({ newWindow }) => newWindow(exported.path)),
        });
      } else {
        this.say("Word and LibreOffice aren’t installed, so Letters prints its own pages: choose “Save as PDF” or “Microsoft Print to PDF”. Page breaks may differ from Word’s.");
        setTimeout(() => window.print(), 300);
      }
    } catch (problem) {
      this.notice = null;
      this.onError(errorText(problem));
    }
  }

  // --- pictures: in line or free, size, pasting

  /** the zoom the page is shown at, set by the screen (for screen to page sizes) */
  zoom = 1;

  /** In Line or Free for pictures the engine can't reach (pictureCommands.ts) */
  setImageFreeLocally(free: boolean) {
    pictures.setImageFreeLocally(this, free);
  }

  /** one undo step for a whole resize or move. */
  setImageGeometry(change: (data: Data) => Data) {
    this.run((s) => edit.updateImage(s, change), false);
  }

  /** pasted or dropped picture files, through the engine (pictureCommands.ts) */
  insertPictureFiles(files: File[], pos?: number) {
    return pictures.insertPictureFiles(this, files, pos);
  }

  /** selects a whole node (a text block clicked by its edge), for Delete, Copy and the frame. */
  selectNode(pos: number) {
    this.run((state) => (state.doc.nodeAt(pos) ? state.tr.setSelection(NodeSelection.create(state.doc, pos)) : null));
  }

  // --- strict word mode and formats: the engine's tables (strictMode.ts, formats.ts)

  /** the engine's feature table (compat::feature_matrix) */
  features = $state.raw<api.EngineFeature[]>([]);
  /** what Word would show differently in this document (compat::word_compatibility) */
  issues = $state.raw<api.CompatibilityIssue[]>([]);
  /** the formats Save As offers (formats::format_capabilities) */
  formats = $state.raw<SaveFormat[]>([]);

  /** asks the engine what Word would show differently now; returns the issues. */
  async checkCompatibility(): Promise<api.CompatibilityIssue[]> {
    try {
      const answer = await api.compatibility(this.id, this.model());
      this.features = answer.features;
      this.issues = answer.report.issues;
    } catch {
      // the line keeps its last answer; the next edit asks again
    }
    return this.issues;
  }

  /** the engine's fixes for Strict Word mode: every one, or issue number `only`. one undo step. */
  async fixIssues(only?: number) {
    try {
      const answer = await api.objectCommand(this.id, this.model(), only === undefined ? { op: "applyFixes" } : { op: "applyFixes", only });
      this.run((state) => applyEngineModel(state, answer.model), false);
      const left = await this.checkCompatibility();
      this.announce(left.length ? `Fixed. ${left.length} thing${left.length === 1 ? "" : "s"} Letters can’t change ${left.length === 1 ? "is" : "are"} left.` : "Fixed. Everything here opens the same in Word.");
    } catch (problem) {
      this.onError(errorText(problem));
    }
  }

  /** the engine's format table, asked once. */
  async loadFormats(): Promise<SaveFormat[]> {
    if (this.formats.length) return this.formats;
    try {
      const answer = await api.saveFormats();
      this.formats = saveFormatsFrom(answer.formats, answer.word || answer.libreOffice);
    } catch (problem) {
      this.onError(errorText(problem));
    }
    return this.formats;
  }

  // --- counts and status

  counts(): { words: number; selected: number | null } {
    const state = this.editor;
    const words = countDocument(state.doc).words;
    const { from, to, empty } = state.selection;
    return { words, selected: empty || state.selection instanceof NodeSelection ? null : countRange(state.doc, from, to).words };
  }

  language(): string {
    const meta = this.meta;
    return meta.metadata?.language || (meta.styles?.defaults?.run?.language as string | undefined) || this.settings.typing.language || "en-US";
  }

  async setSettings(next: api.WriterSettings) {
    this.settings = next;
    try {
      this.settings = await api.saveWriterSettings(next);
    } catch {
      // kept for this session even if the file could not be written
    }
  }

  setTabStops(tabs: TabStop[]) {
    this.run((s) => edit.updateParagraphs(s, (props) => ({ ...props, tabs: tabs.length ? tabs : undefined })), false);
  }
}

/** the shell's kind of error ("changed-on-disk", "failed"…), if it gave one. */
export function errorKind(problem: unknown): string | null {
  if (typeof problem === "object" && problem && "kind" in problem) return String((problem as { kind: unknown }).kind);
  return null;
}

export function errorText(problem: unknown): string {
  if (typeof problem === "object" && problem && "message" in problem) return String((problem as { message: unknown }).message);
  return String(problem);
}

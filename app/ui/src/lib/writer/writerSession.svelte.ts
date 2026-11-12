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
import { freeOffsets } from "./resize";

type Data = Record<string, unknown>;

export interface WriterNotice {
  text: string;
  action?: { label: string; run: () => void };
}

export class WriterSession {
  id: number;
  name = $state("");
  path = $state("");
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
    this.editor = PmEditorState.create({ doc: modelToDoc(opened.model), plugins: this.plugins() });
    this.saved = this.editor.doc;
    this.autosaved = this.editor.doc;
    this.tracking = !!opened.model.settings?.trackChanges;
    this.changeIds = nextChangeId(this.editor.doc);
    api.writerSettings().then((settings) => (this.settings = settings)).catch(() => {});
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
      // pictures pasted or dropped from other apps and from the system
      handlePaste: (_view, event) => {
        const files = [...(event.clipboardData?.files ?? [])].filter((file) => file.type.startsWith("image/"));
        if (files.length === 0) return false;
        this.insertPictureFiles(files);
        return true;
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
    return !!this.saved && !this.editor.doc.eq(this.saved);
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

  async save(path: string | null = null): Promise<boolean> {
    const model = this.model();
    const doc = this.view?.state.doc ?? this.editor.doc;
    try {
      const saved = await api.saveWriterDocument(this.id, model, path);
      this.name = saved.name;
      this.path = saved.path;
      this.kind = saved.kind;
      this.saved = doc;
      this.autosaved = doc;
      this.announce(`Saved “${saved.name}”.`);
      return true;
    } catch (problem) {
      this.onError(errorText(problem));
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

  async insertPicture() {
    const path = await api.choosePicture();
    if (!path) return;
    try {
      const { mediaId, url } = await api.addPicture(this.id, path);
      this.media.remember(mediaId, url);
      const size = await pictureSize(url);
      // at most the text width, keeping the proportions
      const section = this.meta.sections?.[0];
      const textWidth = section ? section.page.width - section.page.margins.left - section.page.margins.right : 9360;
      const width = Math.min(size.width * 15, textWidth);
      const height = Math.round((width * size.height) / Math.max(1, size.width));
      this.run((s) => edit.insertImage(s, mediaId, Math.round(width), height, path.split(/[\\/]/).pop() ?? "Picture"));
      this.say("Picture added. Add alt text for it in Format, so screen readers can describe it.");
    } catch (problem) {
      this.onError(errorText(problem));
    }
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

  /** changes the table around the cursor: its data, or each selected cell's. */
  updateTable(change: (data: Data) => Data) {
    this.run((state) => {
      const here = state.selection.$from;
      for (let depth = here.depth; depth > 0; depth -= 1) {
        const node = here.node(depth);
        if (node.type.name === "table") return state.tr.setNodeMarkup(here.before(depth), undefined, { data: change({ ...(node.attrs.data as Data) }) });
      }
      return null;
    });
  }

  updateCells(change: (data: Data) => Data) {
    this.run((state) => {
      const tr = state.tr;
      const { from, to } = state.selection;
      let touched = false;
      state.doc.nodesBetween(Math.min(from, to), Math.max(from, to), (node, pos) => {
        if (node.type.name === "table_cell" || node.type.name === "table_header") {
          tr.setNodeMarkup(pos, undefined, { ...node.attrs, data: change({ ...(node.attrs.data as Data) }) });
          touched = true;
          return false;
        }
        return true;
      });
      if (!touched) {
        const here = state.selection.$from;
        for (let depth = here.depth; depth > 0; depth -= 1) {
          const node = here.node(depth);
          if (node.type.name === "table_cell" || node.type.name === "table_header") {
            tr.setNodeMarkup(here.before(depth), undefined, { ...node.attrs, data: change({ ...(node.attrs.data as Data) }) });
            break;
          }
        }
      }
      return tr;
    });
  }

  setHeaderRow(on: boolean) {
    this.run((state) => {
      const here = state.selection.$from;
      for (let depth = here.depth; depth > 0; depth -= 1) {
        const node = here.node(depth);
        if (node.type.name === "table") {
          const row = node.firstChild;
          if (!row) return null;
          const rowPos = here.before(depth) + 1;
          return state.tr.setNodeMarkup(rowPos, undefined, { data: { ...(row.attrs.data as Data), header: on } });
        }
      }
      return null;
    });
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

  /** the selected picture's element on screen, and its paragraph's. */
  private imageElements(): { image: HTMLElement; paragraph: HTMLElement } | null {
    const selected = edit.selectedImage(this.editor);
    const dom = selected && (this.view?.nodeDOM(selected.pos) as HTMLElement | null);
    const image = dom?.querySelector("img") ?? null;
    const paragraph = dom?.closest("p, h1, h2, h3, h4, h5, h6, td") as HTMLElement | null;
    return image && paragraph ? { image, paragraph } : null;
  }

  /**
   * In Line or Free, without the picture jumping: Free starts exactly where
   * the picture is; In Line puts it at the text position nearest its middle.
   */
  setImageFree(free: boolean) {
    const view = this.view;
    const selected = edit.selectedImage(this.editor);
    const elements = this.imageElements();
    if (!view || !selected || !elements) {
      this.say("Click a picture first.");
      return;
    }
    const data = selected.node.attrs.data as Data;
    const isFree = (data.placement as { type?: string } | undefined)?.type === "floating";
    if (free === isFree) return;
    const box = elements.image.getBoundingClientRect();
    if (free) {
      const offsets = freeOffsets(box, elements.paragraph.getBoundingClientRect(), this.zoom / 15);
      const placement = { type: "floating", horizontal: { relativeTo: "column", offset: offsets.horizontal }, vertical: { relativeTo: "paragraph", offset: offsets.vertical }, wrap: "square" };
      this.run((s) => edit.updateImage(s, (current) => ({ ...current, placement })), false);
      this.announce("The picture is free: drag it anywhere on the page.");
      return;
    }
    // the text position nearest the picture's middle
    const target = view.posAtCoords({ left: box.left + box.width / 2, top: box.top + box.height / 2 });
    this.run((state) => {
      const tr = state.tr.delete(selected.pos, selected.pos + selected.node.nodeSize);
      const at = target ? tr.mapping.map(target.pos) : selected.pos;
      const inline = selected.node.type.create({ data: { ...data, placement: { type: "inline" } } }, null, selected.node.marks);
      tr.insert(at, inline);
      return tr.setSelection(NodeSelection.create(tr.doc, at));
    }, false);
    this.announce("The picture is in line with the text.");
  }

  /** one undo step for a whole resize or move. */
  setImageGeometry(change: (data: Data) => Data) {
    this.run((s) => edit.updateImage(s, change), false);
  }

  /** pastes or drops picture files at the cursor (or at `pos`). */
  async insertPictureFiles(files: File[], pos?: number) {
    for (const file of files.filter((each) => each.type.startsWith("image/"))) {
      try {
        const bytes = new Uint8Array(await file.arrayBuffer());
        let binary = "";
        for (let i = 0; i < bytes.length; i += 0x8000) binary += String.fromCharCode(...bytes.subarray(i, i + 0x8000));
        const name = file.name || `Pasted picture.${file.type.split("/")[1] ?? "png"}`;
        const { mediaId, url } = await api.addPictureData(this.id, name, btoa(binary));
        this.media.remember(mediaId, url);
        const size = await pictureSize(url);
        const section = this.meta.sections?.[0];
        const textWidth = section ? section.page.width - section.page.margins.left - section.page.margins.right : 9360;
        const width = Math.min(size.width * 15, textWidth);
        const height = Math.round((width * size.height) / Math.max(1, size.width));
        this.run((state) => {
          const tr = pos !== undefined ? state.tr.setSelection(TextSelection.near(state.doc.resolve(Math.min(pos, state.doc.content.size)))) : state.tr;
          const image = state.schema.nodes.image.create({ data: { mediaId, width: Math.round(width), height, name, placement: { type: "inline" } } });
          return tr.replaceSelectionWith(image);
        });
        this.announce("Picture pasted.");
      } catch (problem) {
        this.onError(errorText(problem));
      }
    }
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

export function errorText(problem: unknown): string {
  if (typeof problem === "object" && problem && "message" in problem) return String((problem as { message: unknown }).message);
  return String(problem);
}

function pictureSize(url: string): Promise<{ width: number; height: number }> {
  return new Promise((resolve) => {
    const image = new Image();
    image.onload = () => resolve({ width: image.naturalWidth || 200, height: image.naturalHeight || 150 });
    image.onerror = () => resolve({ width: 200, height: 150 });
    image.src = url;
  });
}

// everything the Fill & Sign tools share while a document is open: the
// form as the engine reads it, the field being filled, the fields being
// edited, Find Fields' proposals, the kept signature pictures, the page
// tool in use (drawing a field, placing a signature, marking where a
// digital signature goes) and the guard in front of changing a signed
// document.
//
// the viewer makes one of these and hands it to the pages (the fields'
// boxes and editors, see FormsOverlay.svelte) and to the Fill & Sign tab.
// every change goes through the viewer's change queue, so it is one undo
// step and marks the document edited; the engine draws the result, which
// arrives with the normal tile refresh. pure rules live in the modules
// beside this one, where they are unit tested.

import { asCommandError, chooseSaveLocation, documentState, saveDocument, saveDocumentAs, type DocumentState } from "../api";
import type { PageSize } from "../layout";
import { pathBeside, stemOf } from "../pageApi";
import { rectAround } from "../pageTools/gestures";
import type { PagePoint, Rect } from "../pageTools/pagePoints";
import { rectContains } from "../pageTools/pagePoints";
import type { Gesture, MarkStyle, PagePointer, PageTool } from "../pageTools/pageTool";
import { rectangleTool } from "../pageTools/tools.svelte";
import type { ToolName } from "../tools";
import { textAnswer, textChanged, textForEngine, toggledAnswer } from "./fieldEditing";
import {
  addFields,
  deleteSignaturePicture,
  detectFields,
  flattenForm,
  formsOverview,
  keepSignaturePicture,
  placeSignature,
  removeField,
  signaturePictures,
  signDocument,
  updateField,
  type FieldChanges,
  type FieldInfo,
  type FieldValue,
  type FormsOverview,
  type PictureKind,
  type SignaturePicture,
  type SignSettings,
} from "./formsApi";
import { fillField, type ChangedField } from "./formLogicApi";
import type { SigningChoices } from "./signingApi";
import { recalculatedSentence } from "./formRules";
import { NEW_FIELD_SIZE, newFieldSpec, parseChoices, suggestedName, type AddableKind } from "./newFields";
import { acceptAll, moveProposal, newFieldsFrom, proposalsFrom, setKind, setStatus, type Proposal, type ProposalStatus } from "./proposals";
import { fitInField, placedAround, PLACED_SIGNATURE_WIDTH } from "./signaturePicture";
import { afterChoice, mustAsk, type GuardChoice } from "./signatures";
import { fieldLabel, nextEmptyStop, stepStop, stopAnnouncement, tabStops } from "./tabOrder";
import { plainFieldMessage } from "./validation";
import type { DetectedKind } from "./formsApi";

/** the page tools of Fill & Sign. "none" leaves the pages to Select and the Annotate tools. */
export type FillSignTool = "none" | "drawField" | "placeSignature" | "signArea";

/** what the viewer lends the session. */
export interface FillSignHost {
  documentId: () => number;
  document: () => DocumentState;
  pageSize: (page: number) => PageSize | undefined;
  currentPage: () => number;
  /** the middle of what is on screen: where keyboard placement puts things */
  viewCentre: () => PagePoint;
  /** runs a change through the window's queue; false when it failed or was cancelled */
  change: (action: () => Promise<DocumentState>) => Promise<boolean>;
  /** shows a state the session got by itself (after Save a Copy) */
  accept: (state: DocumentState) => void;
  announce: (text: string) => void;
  reveal: (page: number, rect: Rect) => void;
  notify: (text: string) => void;
  showError: (message: string) => void;
  /** gives the keyboard back to the document */
  focusDocument: () => void;
  /** opens the inspector at a tool's section */
  openTool: (tool: ToolName) => void;
  /** another set of page tools must let go: a Fill & Sign tool is taking over */
  takePageTool: () => void;
}

/** the field being filled, at one of its places on the pages. */
export interface ActiveField {
  field: string;
  widget: number;
}

/** where a field being edited is, while it is dragged or nudged. */
export interface FieldPreview extends ActiveField {
  rect: Rect;
}

const OUTLINE: MarkStyle = { stroke: "rgb(0 136 255)", fill: "rgb(0 136 255 / 0.08)", width: 1, opacity: 1, dash: [4, 3] };

export class FillSignSession {
  // declared first: the derived values below read them
  #host: FillSignHost;
  #revision = -1;
  #acknowledged = false;
  #bypassGuard = false;
  #tools: Record<Exclude<FillSignTool, "none">, PageTool>;

  overview = $state<FormsOverview | null>(null);
  /** the fields are shown as tinted boxes on the pages */
  showFields = $state(true);
  /** filling answers, or editing the fields themselves */
  mode = $state<"fill" | "edit">("fill");
  active = $state<ActiveField | null>(null);
  /** goes up when the active field should take the keyboard focus */
  focusTick = $state(0);
  /** the engine's refusal for a field, in plain words, shown beside it */
  errors = $state<Record<string, string>>({});
  proposals = $state<Proposal[]>([]);
  selectedProposal = $state<string | null>(null);
  selectedField = $state<ActiveField | null>(null);
  framePreview = $state<FieldPreview | null>(null);
  tool = $state<FillSignTool>("none");
  newKind = $state<AddableKind>("text");
  newChoices = $state("Choice 1\nChoice 2");
  pictures = $state<SignaturePicture[]>([]);
  chosenPicture = $state<string | null>(null);
  /** where a visible digital signature goes, drawn on a page */
  signArea = $state<{ page: number; rect: Rect } | null>(null);
  /** a signature field clicked on the page, to be signed */
  requestedSignField = $state<string | null>(null);
  /** the signed-document question while it is showing */
  question = $state<{ answer: (choice: GuardChoice) => void } | null>(null);
  bannerDismissed = $state(false);
  /** a press is under way on a page with one of these tools */
  dragging = $state(false);
  /** how the last field was reached: a click opens a pop-up menu's choices at once */
  lastPress: "pointer" | "keyboard" = "keyboard";

  readonly fields = $derived(this.overview?.fields ?? []);
  readonly stops = $derived(tabStops(this.fields, this.overview?.tabOrders ?? []));
  readonly signatures = $derived(this.overview?.signatures ?? []);
  readonly picture = $derived(this.pictures.find((picture) => picture.id === this.chosenPicture) ?? null);
  readonly pageTool: PageTool | null = $derived(this.#toolFor(this.tool));

  constructor(host: FillSignHost) {
    this.#host = host;
    this.#tools = this.#makeTools();
  }

  // ---- the page tools ----

  #makeTools(): Record<Exclude<FillSignTool, "none">, PageTool> {
    const pageSize = (page: number) => this.#host.pageSize(page);
    const placing = rectangleTool({
      id: "place-signature",
      style: () => OUTLINE,
      pageSize,
      aspect: () => this.#aspect(),
      clickSize: () => ({ width: PLACED_SIGNATURE_WIDTH, height: PLACED_SIGNATURE_WIDTH / this.#aspect() }),
      minSize: 6,
      onDraw: (page, rect) => this.placeSignatureAt(page, rect),
    });
    return {
      drawField: rectangleTool({
        id: "form-field",
        style: () => OUTLINE,
        pageSize,
        clickSize: () => NEW_FIELD_SIZE[this.newKind],
        minSize: 6,
        onDraw: (page, rect) => this.addFieldAt(page, rect),
      }),
      // a press on an empty signature field puts the picture inside it
      placeSignature: withFieldPick(placing, (pointer) => this.#signatureFieldAt(pointer)),
      signArea: rectangleTool({
        id: "signature-area",
        style: () => OUTLINE,
        pageSize,
        clickSize: () => NEW_FIELD_SIZE.signature,
        minSize: 12,
        onDraw: (page, rect) => {
          this.signArea = { page, rect };
          this.setTool("none");
          this.#host.announce(`The signature will go on page ${page + 1}.`);
        },
      }),
    };
  }

  #toolFor(tool: FillSignTool): PageTool | null {
    return tool === "none" ? null : this.#tools[tool];
  }

  #aspect(): number {
    const picture = this.picture;
    return picture && picture.height > 0 ? picture.width / picture.height : 3;
  }

  /** an empty signature field under the pointer, to place a picture into. */
  #signatureFieldAt(pointer: PagePointer): (() => void) | null {
    for (const field of this.fields) {
      if (field.kind !== "signature" || field.signed) continue;
      const widget = field.widgets.find((place) => place.page === pointer.page && rectContains(place.rect, pointer));
      if (widget && widget.page !== null) {
        const page = widget.page;
        return () => this.placeSignatureAt(page, fitInField(this.#aspect(), widget.rect));
      }
    }
    return null;
  }

  /** makes one of the Fill & Sign tools the active page tool, or none. */
  setTool(tool: FillSignTool) {
    if (tool === this.tool) return;
    this.pageTool?.finish?.();
    if (tool !== "none") {
      this.#host.takePageTool();
      // a selected field's frame would catch the presses meant for the tool
      this.selectedField = null;
      this.selectedProposal = null;
    }
    this.tool = tool;
  }

  // ---- what is shown ----

  /** after every change: the form is read again for the new version. */
  setRevision(revision: number) {
    if (revision === this.#revision) return;
    this.#revision = revision;
    this.reload();
  }

  reload() {
    const revision = this.#revision;
    formsOverview(this.#host.documentId())
      .then((overview) => {
        if (revision !== this.#revision) return;
        this.overview = overview;
        // a field that is gone (undo, flatten) can't stay selected
        const names = new Set(overview.fields.map((field) => field.name));
        if (this.active && !names.has(this.active.field)) this.active = null;
        if (this.selectedField && !names.has(this.selectedField.field)) this.selectedField = null;
        this.framePreview = null;
      })
      .catch(() => {
        // a document whose form can't be read just shows no fields
        if (revision === this.#revision) this.overview = null;
      });
  }

  field(name: string): FieldInfo | undefined {
    return this.fields.find((field) => field.name === name);
  }

  // ---- the guard in front of changing a signed document ----

  /**
   * asked by the viewer before every change. a signed document asks the
   * person first (Save a Copy, Edit Anyway, Cancel), once per document.
   * undo and redo never ask.
   */
  async mayChange(isHistory: boolean): Promise<boolean> {
    if (this.#bypassGuard) return true;
    const state = { protected: this.overview?.protected ?? false, acknowledged: this.#acknowledged };
    if (!mustAsk(state, isHistory)) return true;
    const choice = await new Promise<GuardChoice>((resolve) => (this.question = { answer: resolve }));
    this.question = null;
    const after = afterChoice(state, choice);
    if (!after.proceed) {
      this.#host.focusDocument();
      return false;
    }
    if (after.saveCopyFirst && !(await this.saveCopy())) return false;
    this.#acknowledged = after.state.acknowledged;
    return true;
  }

  /**
   * saves the document as a copy; the signed file stays exactly as it is.
   * the copy is still signed, so its own changes are saved after the
   * signature as well (shell-core's forms/signed.rs).
   */
  async saveCopy(): Promise<boolean> {
    const doc = this.#host.document();
    const path = await chooseSaveLocation(pathBeside(doc.path, `${stemOf(doc.path)} (edited).pdf`));
    if (!path) return false;
    try {
      const saved = await saveDocumentAs(doc.id, path);
      this.#host.accept(saved);
      return true;
    } catch (problem) {
      this.#host.showError(asCommandError(problem).message);
      return false;
    }
  }

  // ---- filling ----

  /** goes to a field: scrolls it into view, gives it the focus and says what it is. */
  focusField(name: string, widget?: number, how: "pointer" | "keyboard" = "keyboard") {
    const field = this.field(name);
    if (!field) return;
    this.lastPress = how;
    const stop = this.stops.find((each) => each.field === name);
    const index = widget ?? stop?.widget ?? 0;
    const place = field.widgets[index];
    this.mode = "fill";
    this.showFields = true;
    this.active = { field: name, widget: index };
    this.focusTick += 1;
    if (place && place.page !== null) this.#host.reveal(place.page, place.rect);
    this.#host.announce(stopAnnouncement(this.stops, field));
  }

  /** Tab and Shift Tab: the next or previous field, across pages. */
  tab(from: string, backwards: boolean) {
    const next = stepStop(this.stops, from, backwards);
    if (next) this.focusField(next.field, next.widget);
  }

  /** the next field without an answer. false when every field has one. */
  nextEmpty(): boolean {
    const next = nextEmptyStop(this.stops, this.fields, this.active?.field ?? null);
    if (!next) {
      this.#host.announce(this.stops.length > 0 ? "Every field has an answer." : "This document has no fields to fill.");
      this.#host.notify(this.stops.length > 0 ? "Every field has an answer." : "This document has no fields to fill.");
      return false;
    }
    this.focusField(next.field, next.widget);
    return true;
  }

  /** the field lets go of the keyboard (Escape, or the focus left the page). */
  leave() {
    this.active = null;
  }

  /** stores typed text, unless it is what the field already holds. */
  async commitText(name: string, text: string): Promise<boolean> {
    const field = this.field(name);
    if (!field) return false;
    const answer = textForEngine(field, text);
    if (!textChanged(field, answer)) {
      this.#clearError(name);
      return true;
    }
    return this.#setValue(field, textAnswer(answer));
  }

  /** a click or Space on a checkbox or radio button. */
  async toggle(name: string, widget: number): Promise<boolean> {
    const field = this.field(name);
    if (!field || field.readOnly) return false;
    const answer = toggledAnswer(field, widget);
    return answer ? this.#setValue(field, answer) : true;
  }

  /** the choice of a pop-up menu or list. */
  async choose(name: string, values: string[]): Promise<boolean> {
    const field = this.field(name);
    if (!field) return false;
    const same = values.length === field.selected.length && values.every((value, index) => value === field.selected[index]);
    if (same) return true;
    return this.#setValue(field, { kind: "selected", value: values });
  }

  /**
   * one answer through the change queue. a refusal from the engine is
   * kept beside the field rather than shown as a window message, so the
   * change itself hands back the document as it was.
   */
  async #setValue(field: FieldInfo, value: FieldValue): Promise<boolean> {
    const id = this.#host.documentId();
    let refusal: string | null = null;
    let changed: ChangedField[] = [];
    const worked = await this.#host.change(async () => {
      try {
        // the field's format and checks run first, then every calculation again
        const filled = await fillField(id, field.name, value);
        changed = filled.changed;
        return filled.document;
      } catch (problem) {
        refusal = asCommandError(problem).message;
        return documentState(id);
      }
    });
    if (refusal !== null) {
      const message = plainFieldMessage(refusal, field);
      this.errors = { ...this.errors, [field.name]: message };
      this.#host.announce(`${fieldLabel(field)}: ${message}`);
      return false;
    }
    if (!worked) return false;
    this.#clearError(field.name);
    this.#patchField(field.name, value);
    this.#patchCalculated(field.name, changed);
    return true;
  }

  /** shows the totals the engine worked out again at once, and says them. */
  #patchCalculated(filled: string, changed: ChangedField[]) {
    const calculated = changed.filter((each) => each.name !== filled);
    for (const each of calculated) this.#patchField(each.name, { kind: "text", value: each.value });
    const said = recalculatedSentence(calculated, (name) => this.field(name));
    if (said) this.#host.announce(said);
  }

  #clearError(name: string) {
    if (!(name in this.errors)) return;
    const { [name]: _gone, ...rest } = this.errors;
    this.errors = rest;
  }

  /** shows a new answer at once in the list and the progress line; the engine's own reading follows. */
  #patchField(name: string, value: FieldValue) {
    if (!this.overview) return;
    const fields = this.overview.fields.map((field) => {
      if (field.name !== name) return field;
      if (value.kind === "checked") return { ...field, checked: value.value, value: value.value ? (field.widgets[0]?.onValue ?? "Yes") : null };
      if (value.kind === "selected") return { ...field, selected: value.value, value: value.value[0] ?? null };
      return { ...field, value: value.value, selected: value.value ? [value.value] : [] };
    });
    this.overview = { ...this.overview, fields };
  }

  // ---- editing fields ----

  setMode(mode: "fill" | "edit") {
    this.mode = mode;
    this.active = null;
    if (mode === "fill") this.selectedField = null;
    if (mode === "edit") this.showFields = true;
  }

  selectField(name: string, widget = 0) {
    this.mode = "edit";
    this.selectedField = { field: name, widget };
    this.framePreview = null;
  }

  /** a field drawn on the page with Add Field. */
  async addFieldAt(page: number, rect: Rect): Promise<boolean> {
    const kind = this.newKind;
    const name = suggestedName(kind, this.fields.map((field) => field.name));
    const spec = newFieldSpec(kind, page, rect, name, parseChoices(this.newChoices));
    const id = this.#host.documentId();
    let made: string[] = [];
    this.setTool("none");
    const worked = await this.#host.change(async () => {
      const edit = await addFields(id, [spec]);
      made = edit.names;
      return edit.document;
    });
    if (!worked || made.length === 0) return false;
    this.selectField(made[0]);
    this.#host.announce(`Field “${made[0]}” added to page ${page + 1}.`);
    return true;
  }

  /** Add Field from the keyboard: a field of the usual size in the middle of the view. */
  addFieldFromKeyboard() {
    const centre = this.#host.viewCentre();
    const size = this.#host.pageSize(centre.page);
    if (!size) return;
    const { width, height } = NEW_FIELD_SIZE[this.newKind];
    this.addFieldAt(centre.page, rectAround(centre, width, height, size));
  }

  /** moves or resizes one place of a field. */
  async moveField(name: string, widget: number, rect: Rect): Promise<boolean> {
    this.framePreview = { field: name, widget, rect };
    const changed = await this.changeField(name, { widget, rect });
    if (!changed) this.framePreview = null;
    return changed;
  }

  /** renames a field or changes its settings. returns false when the engine refused (the message is shown). */
  async changeField(name: string, changes: FieldChanges): Promise<boolean> {
    const id = this.#host.documentId();
    let newName = name;
    const worked = await this.#host.change(async () => {
      const edit = await updateField(id, name, changes);
      newName = edit.name;
      return edit.document;
    });
    if (worked && this.selectedField?.field === name) this.selectedField = { ...this.selectedField, field: newName };
    return worked;
  }

  async deleteField(name: string): Promise<boolean> {
    const id = this.#host.documentId();
    const field = this.field(name);
    const worked = await this.#host.change(() => removeField(id, name));
    if (!worked) return false;
    if (this.selectedField?.field === name) this.selectedField = null;
    this.#host.announce(`Field “${field ? fieldLabel(field) : name}” deleted.`);
    return true;
  }

  // ---- finding fields on a flat form ----

  /** asks the engine for proposals on some pages. returns how many it found. */
  async findFields(pages: number[]): Promise<number> {
    try {
      const found = await detectFields(this.#host.documentId(), pages);
      this.proposals = proposalsFrom(found);
      this.selectedProposal = null;
      this.mode = "fill";
      const count = this.proposals.length;
      this.#host.announce(count === 0 ? "No fields were found." : `${count} possible ${count === 1 ? "field was" : "fields were"} found.`);
      const first = this.proposals[0];
      if (first) this.#host.reveal(first.page, first.rect);
      return count;
    } catch (problem) {
      this.#host.showError(asCommandError(problem).message);
      return 0;
    }
  }

  acceptAllProposals() {
    this.proposals = acceptAll(this.proposals);
  }

  setProposalStatus(id: string, status: ProposalStatus) {
    this.proposals = setStatus(this.proposals, id, status);
  }

  setProposalKind(id: string, kind: DetectedKind) {
    this.proposals = setKind(this.proposals, id, kind);
  }

  moveProposal(id: string, rect: Rect) {
    this.proposals = moveProposal(this.proposals, id, rect);
  }

  selectProposal(id: string | null) {
    this.selectedProposal = id;
    const proposal = this.proposals.find((each) => each.id === id);
    if (proposal) this.#host.reveal(proposal.page, proposal.rect);
  }

  clearProposals() {
    this.proposals = [];
    this.selectedProposal = null;
  }

  /** turns the accepted proposals into fields, all in one undo step. */
  async addAcceptedFields(): Promise<boolean> {
    const specs = newFieldsFrom(this.proposals);
    if (specs.length === 0) return false;
    const id = this.#host.documentId();
    let made: string[] = [];
    const worked = await this.#host.change(async () => {
      const edit = await addFields(id, specs);
      made = edit.names;
      return edit.document;
    });
    if (!worked) return false;
    this.clearProposals();
    this.#host.announce(`${made.length} ${made.length === 1 ? "field" : "fields"} added.`);
    return true;
  }

  // ---- flattening ----

  async flatten(includeSignatures: boolean): Promise<number> {
    const id = this.#host.documentId();
    let flattened: string[] = [];
    const worked = await this.#host.change(async () => {
      const edit = await flattenForm(id, includeSignatures);
      flattened = edit.flattened;
      return edit.document;
    });
    if (!worked) return 0;
    this.active = null;
    this.selectedField = null;
    return flattened.length;
  }

  // ---- signature pictures ----

  async loadPictures() {
    try {
      this.pictures = await signaturePictures();
      if (!this.picture) this.chosenPicture = this.pictures[0]?.id ?? null;
    } catch {
      this.pictures = [];
    }
  }

  /** keeps a picture the person made, and chooses it for placing. */
  async keepPicture(png: Uint8Array, kind: PictureKind): Promise<boolean> {
    try {
      const kept = await keepSignaturePicture(png, kind);
      this.pictures = [kept, ...this.pictures.filter((picture) => picture.id !== kept.id)];
      this.chosenPicture = kept.id;
      this.#host.announce("Signature kept on this computer.");
      return true;
    } catch (problem) {
      this.#host.showError(asCommandError(problem).message);
      return false;
    }
  }

  async deletePicture(id: string) {
    try {
      await deleteSignaturePicture(id);
      this.pictures = this.pictures.filter((picture) => picture.id !== id);
      if (this.chosenPicture === id) this.chosenPicture = this.pictures[0]?.id ?? null;
      this.#host.announce("Signature deleted from this computer.");
    } catch (problem) {
      this.#host.showError(asCommandError(problem).message);
    }
  }

  async placeSignatureAt(page: number, rect: Rect): Promise<boolean> {
    const chosen = this.chosenPicture;
    if (!chosen) return false;
    const id = this.#host.documentId();
    this.setTool("none");
    const worked = await this.#host.change(() => placeSignature(id, page, rect, chosen));
    if (worked) this.#host.announce(`Signature added to page ${page + 1}. Click it with Select to move or resize it.`);
    return worked;
  }

  /** Place Signature from the keyboard: in the middle of the view. */
  placeSignatureFromKeyboard() {
    const centre = this.#host.viewCentre();
    const size = this.#host.pageSize(centre.page);
    if (!size || !this.picture) return;
    this.placeSignatureAt(centre.page, placedAround(centre, this.#aspect(), size));
  }

  // ---- digital signatures ----

  /** Draw the Place from the keyboard: a signature-sized place in the middle of the view. */
  signAreaFromKeyboard() {
    const centre = this.#host.viewCentre();
    const size = this.#host.pageSize(centre.page);
    if (!size) return;
    const { width, height } = NEW_FIELD_SIZE.signature;
    this.signArea = { page: centre.page, rect: rectAround(centre, width, height, size) };
    this.#host.announce(`The signature will go in the middle of page ${centre.page + 1}.`);
  }

  /** a click on an empty signature field: open Sign with a Certificate with it chosen. */
  requestSignature(field: string) {
    this.requestedSignField = field;
    this.#host.openTool("signWithCertificate");
  }

  /** saves unsaved changes first, since a signature covers the saved file. false when that failed. */
  async saveBeforeSigning(): Promise<boolean> {
    const doc = this.#host.document();
    if (!doc.dirty) return true;
    try {
      this.#host.accept(await saveDocument(doc.id));
      return true;
    } catch (problem) {
      this.#host.showError(asCommandError(problem).message);
      return false;
    }
  }

  /** signs the saved document. the guard isn't asked: signing never rewrites the file. */
  async sign(settings: SignSettings, choices: SigningChoices | null = null): Promise<boolean> {
    const id = this.#host.documentId();
    this.#bypassGuard = true;
    let worked: boolean;
    try {
      worked = await this.#host.change(() => signDocument(id, settings, choices));
    } finally {
      this.#bypassGuard = false;
    }
    if (!worked) return false;
    // a new signature: the next change must ask again
    this.#acknowledged = false;
    this.signArea = null;
    this.requestedSignField = null;
    this.bannerDismissed = false;
    this.#host.announce("Signed and saved.");
    return true;
  }

  // ---- escape ----

  /** drops what is most in the way, one thing per press. false when nothing was left. */
  escape(): boolean {
    if (this.question) return false;
    if (this.tool !== "none") this.setTool("none");
    else if (this.selectedProposal) this.selectedProposal = null;
    else if (this.selectedField) this.selectedField = null;
    else if (this.active) this.active = null;
    else return false;
    return true;
  }
}

/**
 * a page tool that first asks whether a press is on something of its
 * own (an empty signature field): then a click there does that instead
 * of what the tool would do.
 */
function withFieldPick(tool: PageTool, pick: (pointer: PagePointer) => (() => void) | null): PageTool {
  return {
    id: tool.id,
    cursor: tool.cursor,
    crossesPages: tool.crossesPages,
    down(pointer): Gesture | null {
      const action = pick(pointer);
      if (!action) return tool.down(pointer);
      return { move() {}, up: () => action(), cancel() {} };
    },
    marks: (page) => tool.marks(page),
    finish: () => tool.finish?.(),
  };
}

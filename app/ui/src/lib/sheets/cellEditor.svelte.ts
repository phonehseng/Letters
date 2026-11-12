// typing into a cell, in the cell itself or in the formula bar (both show
// this one text). like excel it has two modes: "enter" mode, started by
// typing over a cell, where the arrows finish the entry (or point at
// cells while a formula expects a reference), and "edit" mode, started by
// F2 or a double-click, where the arrows move the cursor in the text.
// autocomplete, the argument hint and the reference colours come from the
// engine's help after each key.

import { acceptFunction, expectsReference, pointedReference, withPointedReference, wordBeforeCaret } from "./formulaEntry";
import {
  argumentHint,
  autoClose,
  completeFunctions,
  cycleReference,
  referenceSpans,
  type ArgumentHint,
  type CellRef,
  type ColouredReference,
  type Completion,
  type Range,
} from "./sheetsApi";

export type EditMode = "enter" | "edit";

/** how many functions autocomplete lists, as excel's list shows about this many */
const COMPLETIONS_SHOWN = 8;

export interface Pointing {
  /** the formula as it was before the pointed reference went in */
  base: string;
  caret: number;
  /** the corner that stays while shift and an arrow stretch the pointed range */
  from: CellRef;
  /** the corner the arrows move */
  to: CellRef;
}

export class CellEditor {
  active = $state(false);
  at = $state<CellRef>({ row: 0, col: 0 });
  sheet = $state(0);
  text = $state("");
  caret = $state(0);
  mode = $state<EditMode>("enter");
  /** "bar" when typing in the formula bar: its field has the focus */
  place = $state<"cell" | "bar">("cell");
  completions = $state<Completion[]>([]);
  highlighted = $state(0);
  hint = $state<ArgumentHint | null>(null);
  references = $state<ColouredReference[]>([]);
  pointing = $state<Pointing | null>(null);
  /** the answers of older keys are dropped when a newer key has been typed */
  private ticket = 0;

  begin(at: CellRef, sheet: number, text: string, mode: EditMode, place: "cell" | "bar" = "cell") {
    this.active = true;
    this.at = at;
    this.sheet = sheet;
    this.mode = mode;
    this.place = place;
    this.pointing = null;
    this.update(text, text.length);
  }

  /** a new text from the field (a key, a paste, autocomplete). */
  update(text: string, caret: number) {
    this.text = text;
    this.caret = caret;
    void this.refreshHelp();
  }

  private async refreshHelp() {
    const ticket = ++this.ticket;
    const typing = wordBeforeCaret(this.text, this.caret);
    const formula = this.text.startsWith("=");
    const [completions, hint, references] = await Promise.all([
      typing ? completeFunctions(typing.word, COMPLETIONS_SHOWN) : Promise.resolve([]),
      formula ? argumentHint(this.text, this.caret) : Promise.resolve(null),
      formula ? referenceSpans(this.text) : Promise.resolve([]),
    ]).catch(() => [[], null, []] as [Completion[], null, ColouredReference[]]);
    if (ticket !== this.ticket || !this.active) return;
    this.completions = completions;
    this.highlighted = 0;
    this.hint = hint;
    this.references = references;
  }

  /** Tab (or a click) on a suggestion puts "NAME(" in. */
  acceptCompletion(index = this.highlighted) {
    const chosen = this.completions[index];
    if (!chosen) return;
    const next = acceptFunction(this.text, this.caret, chosen.name);
    this.completions = [];
    this.update(next.text, next.caret);
  }

  /** whether a click on the grid, or an arrow, should point at cells now. */
  wantsReference(): boolean {
    if (this.pointing) return true;
    return this.active && expectsReference(this.text, this.caret);
  }

  /** puts (or moves) the pointed range into the formula. */
  point(range: Range, sheetName: string | null) {
    if (!this.pointing) this.pointing = { base: this.text, caret: this.caret, from: range.start, to: range.end };
    const reference = pointedReference(range, sheetName);
    const next = withPointedReference(this.pointing.base, this.pointing.caret, reference);
    this.text = next.text;
    this.caret = next.caret;
    void this.refreshHelp();
  }

  /** typing after pointing goes on from the end of the reference. */
  stopPointing() {
    this.pointing = null;
  }

  /** F4: A1 → $A$1 → A$1 → $A1 → A1 for the reference at the cursor. */
  async cycle() {
    const next = await cycleReference(this.text, this.caret);
    this.update(next.text, next.caret);
  }

  moveHighlight(step: 1 | -1) {
    const count = this.completions.length;
    if (count > 0) this.highlighted = (this.highlighted + step + count) % count;
  }

  /** the text to put in the cell: formulas get their brackets and quotes closed. */
  async finished(): Promise<string> {
    const text = this.text.startsWith("=") ? await autoClose(this.text).catch(() => this.text) : this.text;
    this.end();
    return text;
  }

  end() {
    this.active = false;
    this.pointing = null;
    this.completions = [];
    this.hint = null;
    this.references = [];
    this.ticket += 1;
  }
}

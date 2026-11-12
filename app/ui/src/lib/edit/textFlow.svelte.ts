// typing that re-wraps a paragraph (or a ruled table cell, whose row
// grows) and moves the column below it down by whole lines, through the
// engine's reflow. the edit session hands a change here when planFlow
// (flowPlan.ts) says the paragraph no longer fits its lines.
//
// when the re-wrapped text runs past the bottom of its column the engine
// changes nothing and answers the ways out; `pending` holds them while the
// overflow sheet asks the person, and the edit is sent again with the
// choice. after a change, the letters that came from another font are said
// beside the text and what moved is said in one quiet line.

import type { DocumentState } from "../api";
import { matchingBlock } from "./blocks";
import { extensionSentence, summarySentence } from "./changeSentences";
import type { ConsentGate } from "./consent.svelte";
import { contentApi, type FlowAnswer, type OverflowChoice, type OverflowReport } from "./contentApi";
import type { PageObjects, TextBlock } from "./editApi";
import { noticeSentence } from "./editSentences";
import type { PageObjectsStore } from "./pageObjectsStore.svelte";
import type { Rect } from "../pageTools/pagePoints";
import { editorText } from "./textDiff";

/** what the edit session lends the reflow. */
export interface FlowHost {
  documentId: () => number;
  /** runs a change through the window's queue; false when it failed (already shown) */
  change: (action: () => Promise<DocumentState>) => Promise<boolean>;
  announce: (text: string) => void;
  notify: (text: string) => void;
  objects: PageObjectsStore;
  keep: (revision: number, objects: PageObjects, stale: boolean) => void;
  consent: ConsentGate;
  /** shows sentences beside the text just changed */
  showNotice: (page: number, rect: Rect, sentences: string[]) => void;
  /** opens the editor again with what was typed, after the person backed out */
  reopen: (page: number, block: TextBlock, text: string) => void;
}

/** a reflow waiting for the person's choice. */
export interface PendingOverflow {
  page: number;
  block: TextBlock;
  text: string;
  report: OverflowReport;
}

export class TextFlow {
  #host: FlowHost;

  /** the overflow sheet's question, while it shows */
  pending = $state<PendingOverflow | null>(null);

  constructor(host: FlowHost) {
    this.#host = host;
  }

  /** re-sets `block` with `text`; `overflow` says what to do when it doesn't fit. */
  async reflow(page: number, block: TextBlock, text: string, overflow: OverflowChoice = "ask", retry = true): Promise<boolean> {
    const id = this.#host.documentId();
    const consent = this.#host.consent.consent;
    const answer = await this.#run(() => contentApi.reflowText(id, page, block.id, text, { overflow, consent }));
    if (!answer) return false;
    if (answer.stale) return this.#again(page, block, text, overflow, answer.objects, retry);
    if (!answer.applied) {
      if (answer.overflow) this.pending = { page, block, text, report: answer.overflow };
      return false;
    }
    this.#after(page, block, answer);
    return true;
  }

  /** the paragraph changed under the editor: it is found again by place, once, if its words are the same. */
  #again(page: number, block: TextBlock, text: string, overflow: OverflowChoice, objects: PageObjects, retry: boolean): Promise<boolean> {
    const again = matchingBlock(objects, block.rect);
    if (!retry || !again || editorText(again) !== editorText(block)) {
      this.#host.notify("That text changed while you were typing, so nothing was changed. Click it to edit it again.");
      return Promise.resolve(false);
    }
    return this.reflow(page, again, text, overflow, false);
  }

  /** the overflow sheet's answer. */
  choose(choice: Exclude<OverflowChoice, "ask">): Promise<boolean> {
    const pending = this.pending;
    this.pending = null;
    if (!pending) return Promise.resolve(false);
    // nothing changed while the sheet showed, so the block's id still holds
    return this.reflow(pending.page, pending.block, pending.text, choice);
  }

  /** Cancel on the overflow sheet: nothing changes and the typing is kept. */
  cancel() {
    const pending = this.pending;
    this.pending = null;
    if (pending) this.#host.reopen(pending.page, pending.block, pending.text);
  }

  async #run(call: () => Promise<FlowAnswer>): Promise<FlowAnswer | null> {
    let answer: FlowAnswer | null = null;
    const worked = await this.#host.change(async () => {
      const answered = await call();
      answer = answered;
      this.#host.keep(answered.document.revision, answered.objects, answered.stale);
      return answered.document;
    });
    return worked ? answer : null;
  }

  #after(page: number, block: TextBlock, answer: FlowAnswer) {
    const sentences = [...answer.notices.map(noticeSentence), ...answer.summary.fontsExtended.map(extensionSentence)];
    const now = answer.blockId ? answer.objects.textBlocks.find((each) => each.id === answer.blockId) : null;
    if (sentences.length > 0) this.#host.showNotice(page, now?.rect ?? block.rect, sentences);
    const quiet = summarySentence(answer.summary, answer.heightChange);
    if (quiet) this.#host.notify(quiet);
    const where = answer.inTable ? "Table cell" : "Paragraph";
    this.#host.announce(`${where} changed on page ${page + 1}. ${quiet}`.trim());
  }
}

// Page Setup for one page: its margins (moving the content), its paper
// size, extra white space on one side, and its five page boxes. the sheet
// (PageSetupSheet.svelte) shows these numbers; each Apply is one change,
// one undo step, and answers the page's layout as it is afterwards, so the
// numbers shown are always the page's own.

import type { DocumentState } from "../api";
import { summarySentence } from "./changeSentences";
import type { ConsentGate } from "./consent.svelte";
import { contentApi, type BoxNumbers, type MarginFit, type Margins, type PageAnswer, type PageBoxes, type PageLayout, type PaperFit, type Side } from "./contentApi";
import type { BoxName } from "./pageSetupWords";

export interface PageSetupHost {
  documentId: () => number;
  change: (action: () => Promise<DocumentState>) => Promise<boolean>;
  announce: (text: string) => void;
  notify: (text: string) => void;
  showError: (message: string) => void;
  consent: ConsentGate;
}

const NO_MARGINS: Margins = { top: 0, right: 0, bottom: 0, left: 0 };

export class PageSetup {
  #host: PageSetupHost;

  open = $state(false);
  page = $state(0);
  layout = $state<PageLayout | null>(null);
  margins = $state<Margins>({ ...NO_MARGINS });
  marginFit = $state<MarginFit>("scale");
  paper = $state({ width: 612, height: 792 });
  paperFit = $state<PaperFit>("fit");
  spaceSide = $state<Side>("bottom");
  spacePoints = $state(36);
  boxes = $state<PageBoxes>({ media: null, crop: null, bleed: null, trim: null, art: null });

  constructor(host: PageSetupHost) {
    this.#host = host;
  }

  /** opens the sheet for `page`, with its numbers as they are. */
  async show(page: number) {
    this.page = page;
    try {
      this.#take(await contentApi.pageLayout(this.#host.documentId(), page));
      this.open = true;
    } catch (problem) {
      this.#host.showError(messageOf(problem));
    }
  }

  close() {
    this.open = false;
  }

  #take(layout: PageLayout) {
    this.layout = layout;
    // measured margins carry the content's exact edges (71.99988); a person types whole points
    const round = (value: number) => Math.round(value * 100) / 100;
    const measured = layout.margins.margins;
    this.margins = { top: round(measured.top), right: round(measured.right), bottom: round(measured.bottom), left: round(measured.left) };
    this.paper = { width: Math.round(layout.margins.width), height: Math.round(layout.margins.height) };
    this.boxes = { ...layout.boxes };
  }

  applyMargins(): Promise<boolean> {
    const margins = { ...this.margins };
    const fit = this.marginFit;
    return this.#apply((id, consent) => contentApi.setMargins(id, this.page, margins, fit, consent), "Margins changed");
  }

  applyPaper(): Promise<boolean> {
    const { width, height } = this.paper;
    const fit = this.paperFit;
    return this.#apply((id, consent) => contentApi.resizePage(id, this.page, width, height, fit, consent), "Paper size changed");
  }

  applySpace(): Promise<boolean> {
    const side = this.spaceSide;
    const points = this.spacePoints;
    return this.#apply((id, consent) => contentApi.addPageSpace(id, this.page, side, points, consent), "Page space changed");
  }

  /** sets one box to the numbers typed. */
  applyBox(name: BoxName, numbers: BoxNumbers): Promise<boolean> {
    return this.#apply((id, consent) => contentApi.setPageBoxes(id, this.page, { [name]: numbers }, consent), "Page box changed");
  }

  async #apply(call: (id: number, consent: { invalidateSignatures: boolean }) => Promise<PageAnswer>, said: string): Promise<boolean> {
    let answer: PageAnswer | null = null;
    const worked = await this.#host.change(async () => {
      const answered = await call(this.#host.documentId(), this.#host.consent.consent);
      answer = answered;
      return answered.document;
    });
    if (!worked || !answer) return false;
    const made: PageAnswer = answer;
    this.#take(made.layout);
    this.#host.announce(`${said} on page ${this.page + 1}.`);
    const quiet = summarySentence(made.summary);
    if (quiet) this.#host.notify(quiet);
    return true;
  }
}

function messageOf(problem: unknown): string {
  if (typeof problem === "object" && problem !== null && "message" in problem) return String((problem as { message: unknown }).message);
  return String(problem);
}

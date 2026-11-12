// what the Scan tab shares while a document is open: whether text
// recognition is installed, which pages are scans (for the banner), the
// choices for the next run, the run under way, the summary afterwards,
// and the recognized words drawn by Show Recognized Text.
//
// a run happens in the shell on its own thread; the session asks how it
// is doing a few times a second, so the document stays usable meanwhile.
// when it has finished, every page's text is added through the viewer's
// change queue as one edit, so one Undo takes it back.

import { SvelteMap } from "svelte/reactivity";
import type { DocumentState } from "../api";
import type { Dispatcher } from "../commands";
import type { ToolName } from "../tools";
import { defaultLanguage, languageNames } from "./languages";
import { ocrApi, optionsFrom, type CleanupChoices, type OcrStatus, type OverlayPage, type ScanOverview } from "./ocrApi";
import { OCR_COMMANDS, type OcrCommand } from "./ocrCommands";
import { IDLE, isBusy, jobOf, nextView, type JobEvent, type JobView } from "./ocrJob";
import { cancelledSentence, summarySentence } from "./ocrSentences";
import { pagesInWords, type PageChoice } from "./pageRange";

export interface ScanHost {
  documentId: () => number;
  pageCount: () => number;
  change: (action: () => Promise<DocumentState>) => Promise<boolean>;
  announce: (text: string) => void;
  notify: (text: string, action?: { label: string; run: () => void }) => void;
  showError: (message: string) => void;
  openTool: (tool: ToolName) => void;
}

/** how often a run is asked how it is doing, in ms. */
const POLL_MS = 350;

/** words tesseract was less sure of than this are tinted in the overlay. */
export const DOUBTFUL_WORD = 70;

export class ScanSession {
  #host: ScanHost;
  #revision = -1;
  #timer: ReturnType<typeof setTimeout> | null = null;
  #loadingOverlays = new Set<number>();
  /** the words a correction answered, for the revision it made */
  #answered: { revision: number; overlay: OverlayPage } | null = null;

  status = $state<OcrStatus | null>(null);
  overview = $state<ScanOverview | null>(null);
  /** the person closed the banner for this document */
  bannerClosed = $state(false);
  languages = $state<string[]>([]);
  cleanup = $state<CleanupChoices>({ orientation: true, deskew: true, denoise: true });
  pageChoice = $state<PageChoice | null>(null);
  view = $state<JobView>(IDLE);
  /** Show Recognized Text is on */
  showText = $state(false);
  readonly overlays = new SvelteMap<number, OverlayPage>();
  /** a word being corrected on the page */
  correcting = $state<{ page: number; index: number } | null>(null);

  readonly unreadScans = $derived(this.overview ? this.overview.scanned.filter((page) => !this.overview!.recognized.includes(page)) : []);
  readonly showBanner = $derived(
    !this.bannerClosed && this.unreadScans.length > 0 && this.status?.available !== false && !isBusy(this.view),
  );
  readonly busy = $derived(isBusy(this.view));

  constructor(host: ScanHost) {
    this.#host = host;
    ocrApi
      .status()
      .then((status) => {
        this.status = status;
        const first = defaultLanguage(status.languages, navigator.language || "en");
        if (first && this.languages.length === 0) this.languages = [first];
      })
      .catch(() => (this.status = { available: false, message: "Text recognition couldn’t be started.", languages: [], canDetectOrientation: false }));
  }

  /** after every change: which pages are scans, and the recognized words, are read again. */
  setRevision(revision: number) {
    if (revision === this.#revision) return;
    this.#revision = revision;
    this.overlays.clear();
    this.#loadingOverlays.clear();
    this.correcting = null;
    const answered = this.#answered;
    this.#answered = null;
    if (answered && answered.revision === revision) this.overlays.set(answered.overlay.page, answered.overlay);
    ocrApi
      .overview(this.#host.documentId())
      .then((overview) => {
        if (revision === this.#revision) this.overview = overview;
      })
      .catch(() => {
        if (revision === this.#revision) this.overview = null;
      });
  }

  attach(commands: Dispatcher): () => void {
    const handlers: Record<OcrCommand, () => void> = {
      "make-searchable": () => this.#host.openTool("recognizeText"),
      "show-recognized-text": () => this.toggleText(),
      "remove-recognized-text": () => this.#host.openTool("removeRecognizedText"),
    };
    const takeBack = OCR_COMMANDS.map((command) => commands.register(command, handlers[command]));
    return () => {
      takeBack.forEach((unregister) => unregister());
      this.#stopPolling();
    };
  }

  languageWords(): string {
    return languageNames(this.languages, this.status?.languages ?? []);
  }

  // ---- a run ----

  #send(event: JobEvent) {
    this.view = nextView(this.view, event);
  }

  async start(pages: number[]): Promise<void> {
    if (this.busy || pages.length === 0) return;
    this.#send({ type: "start" });
    const id = this.#host.documentId();
    try {
      const job = await ocrApi.start(id, pages, optionsFrom(this.languages, this.cleanup));
      this.#send({ type: "started", job, total: pages.length });
      this.#host.announce(`Reading ${pagesInWords(pages, this.#host.pageCount())}.`);
      this.#poll();
    } catch (problem) {
      this.#send({ type: "failed", message: messageOf(problem) });
    }
  }

  /** stops the run; tesseract is stopped at once and nothing is added. */
  async cancel() {
    const job = jobOf(this.view);
    if (job === null || this.view.phase !== "running") return;
    this.#send({ type: "cancel" });
    await ocrApi.cancel(job).catch(() => false);
  }

  /** back to the choices after a run ended. */
  reset() {
    this.#send({ type: "reset" });
  }

  #poll() {
    this.#stopPolling();
    this.#timer = setTimeout(() => this.#check(), POLL_MS);
  }

  #stopPolling() {
    if (this.#timer) clearTimeout(this.#timer);
    this.#timer = null;
  }

  async #check() {
    const job = jobOf(this.view);
    if (job === null) return;
    const doneBefore = this.view.phase === "running" ? this.view.done : -1;
    try {
      const status = await ocrApi.jobStatus(job);
      this.#send({ type: "status", status });
    } catch (problem) {
      this.#send({ type: "failed", message: messageOf(problem) });
      return;
    }
    const view = this.view;
    if (view.phase === "running" && doneBefore >= 0 && view.done > doneBefore) this.#host.announce(`${view.done} of ${view.total} pages read.`);
    if (view.phase === "running" || view.phase === "cancelling") return this.#poll();
    if (view.phase === "applying") return this.#apply(view.job);
    if (view.phase === "cancelled") {
      ocrApi.discard(job).catch(() => {});
      this.#host.announce(cancelledSentence(view));
    }
    if (view.phase === "failed") ocrApi.discard(job).catch(() => {});
  }

  async #apply(job: number) {
    const id = this.#host.documentId();
    let summary = null as Awaited<ReturnType<typeof ocrApi.apply>>["summary"] | null;
    const worked = await this.#host.change(async () => {
      const applied = await ocrApi.apply(id, job);
      summary = applied.summary;
      return applied.document;
    });
    if (!worked || !summary) {
      this.#send({ type: "failed", message: "The text was read but couldn’t be added. Choose Recognize Text again." });
      return;
    }
    this.#send({ type: "applied", summary });
    this.#host.announce(summarySentence(summary));
  }

  // ---- after a run ----

  async removeText(pages: number[]): Promise<boolean> {
    const id = this.#host.documentId();
    const worked = await this.#host.change(() => ocrApi.removeText(id, pages));
    if (worked) this.#host.announce(`Recognized text removed from ${pagesInWords(pages, this.#host.pageCount())}.`);
    return worked;
  }

  toggleText() {
    this.showText = !this.showText;
    this.correcting = null;
    this.#host.announce(this.showText ? "Recognized text shown." : "Recognized text hidden.");
  }

  /** the recognized words of the pages on screen, read once per revision. */
  ensureOverlays(pages: number[]) {
    if (!this.showText || !this.overview) return;
    const revision = this.#revision;
    const id = this.#host.documentId();
    for (const page of pages) {
      if (!this.overview.recognized.includes(page) || this.overlays.has(page) || this.#loadingOverlays.has(page)) continue;
      this.#loadingOverlays.add(page);
      ocrApi
        .overlay(id, page)
        .then((overlay) => {
          if (revision === this.#revision) this.overlays.set(page, overlay);
        })
        .catch(() => {})
        .finally(() => this.#loadingOverlays.delete(page));
    }
  }

  /** rewrites one recognized word in the page's text layer, as one undo step. */
  async correctWord(page: number, index: number, text: string): Promise<boolean> {
    this.correcting = null;
    const id = this.#host.documentId();
    const worked = await this.#host.change(async () => {
      const answer = await ocrApi.correctWord(id, page, index, text);
      // kept for the revision the correction made, so it shows without asking again
      this.#answered = { revision: answer.document.revision, overlay: answer.overlay };
      return answer.document;
    });
    if (!worked) return false;
    this.#host.announce(`Word corrected on page ${page + 1}.`);
    return true;
  }
}

function messageOf(problem: unknown): string {
  if (typeof problem === "object" && problem !== null && "message" in problem) return String((problem as { message: unknown }).message);
  return String(problem);
}

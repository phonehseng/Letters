// what the Accessibility tab shares while a document is open: the last
// check's report, the fix wizard, and what is outlined on the page (a
// finding's spot, or the numbered blocks of the reading order step).
//
// the viewer makes one of these beside the other sessions. every fix goes
// through the viewer's change queue (one undo step each); afterwards the
// document is checked again, so the list and the wizard follow the file.

import type { DocumentState } from "../api";
import {
  a11yWords,
  applyFix,
  checkDocument,
  type A11yWords,
  type Finding,
  type FixOutcome,
  type FixRequest,
  type ReadingBlock,
  type Rect,
  type Report,
} from "./a11yApi";
import { advance, back, currentStep, refresh, skip, startWizard, type WizardState } from "./wizard";

/** what the viewer lends the session. */
export interface A11yHost {
  documentId: () => number;
  /** runs a change through the window's queue; false when it failed (the error is already shown) */
  change: (action: () => Promise<DocumentState>) => Promise<boolean>;
  reveal: (page: number, rect: Rect) => void;
  goToPage: (page: number) => void;
  announce: (text: string) => void;
  showError: (message: string) => void;
}

/** a spot outlined on a page: a finding's place. */
export interface Spot {
  page: number;
  rect: Rect;
  label: string;
}

export class A11ySession {
  report = $state<Report | null>(null);
  checking = $state(false);
  /** the revision the report describes; a later revision makes it out of date */
  checkedRevision = $state(-1);
  revision = $state(0);
  wizard = $state<WizardState | null>(null);
  spot = $state<Spot | null>(null);
  /** the reading order step's blocks, numbered on the page while it is open */
  readingOrder = $state<{ page: number; blocks: ReadingBlock[] } | null>(null);
  /** what the last fix did, said under the wizard */
  lastOutcome = $state<FixOutcome | null>(null);
  /** the tagger's decisions, kept for review after Add Tags Automatically */
  decisions = $state<FixOutcome["decisions"]>([]);
  words = $state<A11yWords | null>(null);

  constructor(private readonly host: A11yHost) {}

  get outOfDate(): boolean {
    return this.report !== null && this.checkedRevision !== this.revision;
  }

  /** the viewer reports every new revision; an open wizard follows undo and redo. */
  setRevision(revision: number) {
    this.revision = revision;
    if (this.wizard && this.checkedRevision !== revision && !this.checking) void this.check();
  }

  async loadWords() {
    if (!this.words) this.words = await a11yWords().catch(() => null);
  }

  /** checks the document as it is now. */
  async check(): Promise<Report | null> {
    this.checking = true;
    const revision = this.revision;
    try {
      const report = await checkDocument(this.host.documentId());
      this.report = report;
      this.checkedRevision = revision;
      if (this.wizard) this.wizard = advance(refresh(this.wizard, report));
      this.host.announce(`Check finished. ${report.summary.mustFix} must fix, ${report.summary.shouldFix} should fix.`);
      return report;
    } catch (problem) {
      this.host.showError(messageOf(problem));
      return null;
    } finally {
      this.checking = false;
    }
  }

  /** runs one fix as one edit, then checks again. */
  async fix(request: FixRequest): Promise<boolean> {
    // filled in by the change, which runs when its turn in the queue comes
    const answer: { outcome: FixOutcome | null; revision: number } = { outcome: null, revision: 0 };
    const done = await this.host.change(async () => {
      const edit = await applyFix(this.host.documentId(), request);
      answer.outcome = edit.outcome;
      answer.revision = edit.document.revision;
      return edit.document;
    });
    const outcome = answer.outcome;
    if (!done || !outcome) return false;
    // the viewer passes the new revision on a moment later; the check is of this one
    this.revision = Math.max(this.revision, answer.revision);
    this.lastOutcome = outcome;
    if (request.kind === "addTags") this.decisions = outcome.decisions;
    this.host.announce(outcome.changes[0] ?? "Fixed.");
    // the viewer's state arrives before this line, so the revision is current
    await this.check();
    return true;
  }

  startWizard() {
    if (!this.report) return;
    this.wizard = startWizard(this.report);
    this.lastOutcome = null;
  }

  closeWizard() {
    this.wizard = null;
    this.readingOrder = null;
  }

  skipStep() {
    if (this.wizard) this.wizard = skip(this.wizard);
    this.lastOutcome = null;
  }

  previousStep() {
    if (this.wizard) this.wizard = back(this.wizard);
    this.lastOutcome = null;
  }

  get step() {
    return this.wizard ? currentStep(this.wizard) : null;
  }

  /** goes to a finding's page and outlines its spot. */
  show(finding: Finding) {
    const { page, rect } = finding.where;
    if (page !== null) this.showSpot(page, rect, finding.title);
  }

  /** goes to a page and outlines a place on it (or just goes there). */
  showSpot(page: number, rect: Rect | null, label: string) {
    this.spot = rect ? { page, rect, label } : null;
    this.revealPage(page, rect);
    this.host.announce(`Page ${page + 1}: ${label}`);
  }

  clearSpot() {
    this.spot = null;
  }

  /** brings a page (or a place on it) into view without outlining anything. */
  revealPage(page: number, rect: Rect | null) {
    if (rect) this.host.reveal(page, rect);
    else this.host.goToPage(page);
  }

  showReadingOrder(page: number, blocks: ReadingBlock[]) {
    this.readingOrder = { page, blocks };
  }

  hideReadingOrder() {
    this.readingOrder = null;
  }
}

function messageOf(problem: unknown): string {
  if (typeof problem === "object" && problem !== null && "message" in problem) return String((problem as { message: unknown }).message);
  return String(problem);
}

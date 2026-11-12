// asking Word (Strict Word mode) or Letters' own engine (a setting, Free
// mode) for the document's lines and pages, without ever making typing
// wait. the screen calls `documentChanged` after each edit; the answers
// land in the state fields the page surface and the status bar read.

import { WORD_IDLE_MS, type LayoutProviderName, type WordLayout } from "./layoutSource";
import * as api from "./writerApi";
import { errorText, type WriterSession } from "./writerSession.svelte";

/** the first check after opening goes almost at once; there is nothing to wait for */
const FIRST_CHECK_MS = 300;
/** letters' engine answers in a fraction of a second, so it follows typing closely */
const ENGINE_IDLE_MS = 600;

export class LayoutCheck {
  /** Word's answer for `laidOutDoc` */
  word = $state<WordLayout | null>(null);
  /** the engine's answer for the latest pause in typing */
  engine = $state<WordLayout | null>(null);
  checking = $state(false);
  checkedAt = $state<number | null>(null);
  failed = $state<string | null>(null);
  /** how long the last Word check took, Word's start included */
  lastCheckMs = $state<number | null>(null);
  /** the document Word's answer was asked for */
  laidOutDoc = $state.raw<unknown>(null);
  private wordTimer: ReturnType<typeof setTimeout> | null = null;
  private engineTimer: ReturnType<typeof setTimeout> | null = null;

  constructor(private session: () => WriterSession) {}

  /** the answer to show for a provider. */
  shown(provider: LayoutProviderName): WordLayout | null {
    if (provider === "word") return this.word;
    if (provider === "engine") return this.engine;
    return null;
  }

  /** true when edits came after Word's answer. */
  isStale(provider: LayoutProviderName, doc: unknown): boolean {
    return provider === "word" && this.laidOutDoc !== null && this.laidOutDoc !== doc;
  }

  /** after an edit: Word once typing has paused (a cold Word takes ~21 s), the engine sooner. */
  documentChanged(provider: LayoutProviderName, doc: unknown) {
    if (provider === "word" && doc !== this.laidOutDoc) {
      if (this.wordTimer) clearTimeout(this.wordTimer);
      const wait = this.laidOutDoc === null ? FIRST_CHECK_MS : WORD_IDLE_MS;
      this.wordTimer = setTimeout(() => this.checkWord(), wait);
    }
    if (provider === "engine") {
      if (this.engineTimer) clearTimeout(this.engineTimer);
      this.engineTimer = setTimeout(() => this.checkEngine(), ENGINE_IDLE_MS);
    }
  }

  /** asks Word now (Check Now, or the timer). */
  async checkWord() {
    if (this.checking) return;
    const session = this.session();
    this.laidOutDoc = session.view?.state.doc ?? session.editor.doc;
    this.checking = true;
    this.failed = null;
    const started = performance.now();
    try {
      this.word = await api.wordLayout(session.id, session.model());
      this.lastCheckMs = Math.round(performance.now() - started);
      this.checkedAt = Date.now();
    } catch (problem) {
      this.failed = errorText(problem);
      this.word = null;
    } finally {
      this.checking = false;
    }
  }

  private async checkEngine() {
    const session = this.session();
    try {
      this.engine = await api.engineLayout(session.id, session.model());
    } catch {
      this.engine = null;
    }
  }
}

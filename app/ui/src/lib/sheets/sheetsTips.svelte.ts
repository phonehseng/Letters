// when a tip is offered: only while nobody is typing, a moment after the
// selection settles, one at a time, and not again for a while after one
// was answered, so tips help at the right moment and never nag.

import { book, dismissTip, tipsFor, type CellRef, type RecentAction, type Tip } from "./sheetsApi";
import type { SheetsSession } from "./sheetsSession.svelte";

// how long the selection must stay put before a tip is looked for
const SETTLE_MS = 1500;
// after a tip is answered, the next one waits at least this long
const QUIET_MS = 45_000;
// what the person did, newest last; the engine reads the last few
const RECENT_KEPT = 6;

export class SheetsTips {
  shown = $state<Tip | null>(null);
  private timer: ReturnType<typeof setTimeout> | null = null;
  private quietUntil = 0;
  private recent: RecentAction[] = [];
  /** tips answered in this window: never offered again here */
  private answered = new Set<string>();

  constructor(private readonly session: SheetsSession) {}

  typed(at: CellRef) {
    this.recent = [...this.recent, { typed: { row: at.row, col: at.col } } as RecentAction].slice(-RECENT_KEPT);
  }

  other() {
    this.recent = [...this.recent, "other" as RecentAction].slice(-RECENT_KEPT);
  }

  /** the selection or the workbook changed: the old tip goes, a new one may come. */
  settle(sheet: number, selection: { start: CellRef; end: CellRef }, editing: boolean) {
    if (this.timer) clearTimeout(this.timer);
    this.shown = null;
    if (editing || Date.now() < this.quietUntil) return;
    this.timer = setTimeout(() => void this.look(sheet, selection), SETTLE_MS);
  }

  private async look(sheet: number, selection: { start: CellRef; end: CellRef }) {
    const tips = await tipsFor(this.session.id, sheet, selection, this.recent).catch(() => []);
    this.shown = tips.find((tip) => !this.answered.has(tip.id)) ?? null;
  }

  async apply() {
    const tip = this.answer();
    if (tip) await this.session.apply(tip.operations.map(book), tip.title);
  }

  notNow() {
    const tip = this.answer();
    if (tip) void dismissTip(tip.id, false).catch(() => {});
  }

  never() {
    const tip = this.answer();
    if (tip) void dismissTip(tip.id, true).catch(() => {});
  }

  private answer(): Tip | null {
    const tip = this.shown;
    this.shown = null;
    this.quietUntil = Date.now() + QUIET_MS;
    if (tip) this.answered.add(tip.id);
    return tip;
  }

  stop() {
    if (this.timer) clearTimeout(this.timer);
  }
}

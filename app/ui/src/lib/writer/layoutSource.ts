// where the page breaks on the editing surface come from. three providers,
// best first:
// 1. "word": Microsoft Word lays the document out itself (Strict Word mode,
//    when Word is installed), so lines and pages break exactly where Word
//    breaks them (the line breaks are applied by wordLines.ts)
// 2. "engine": Letters' own layout engine (core/layout), behind a setting
// 3. "estimate": measuring the text on screen (pagination.ts)

import { paginate, type BlockBox, type PageStart, type Pages } from "./pagination";
import type { WordLayoutAnswer } from "./writerApi";

export type LayoutProviderName = "word" | "engine" | "estimate";

export interface ProviderAvailability {
  /** Microsoft Word is installed (asked once from the shell) */
  word: boolean;
  /** Letters' own layout engine is switched on in the settings */
  engine: boolean;
}

export type EditingMode = "free" | "strict";

/** the provider to use: Word only in Strict Word mode, then the engine, then the estimate. */
export function chooseProvider(mode: EditingMode, available: ProviderAvailability): LayoutProviderName {
  if (mode === "strict" && available.word) return "word";
  if (available.engine) return "engine";
  return "estimate";
}

/** Word's answer (shell-core/src/writer/layout.rs); the old form had only the first two fields. */
export type WordLayout = Pick<WordLayoutAnswer, "blockPages" | "pageCount"> & Partial<WordLayoutAnswer>;

/** a page start from Word: a block, and the character offset in it when the page starts mid-paragraph. */
export interface WordPageStart extends PageStart {
  /** the engine's character offset into the paragraph (0 at its start) */
  offset: number;
}

/** where each page starts, from Word's first line on each page (top-level paragraphs only). */
export function wordPageStarts(layout: WordLayout): WordPageStart[] {
  const starts: WordPageStart[] = [{ block: 0, line: 0, offset: 0 }];
  if (layout.lines?.length) {
    let page = 1;
    for (const line of layout.lines) {
      if (line.page <= page) continue;
      page = line.page;
      // a page that starts inside a table starts at the table's next block boundary instead
      if (line.cell) continue;
      starts.push({ block: line.block, line: 0, offset: line.offset });
    }
    return starts;
  }
  layout.blockPages.forEach((page, index) => {
    if (index > 0 && page > layout.blockPages[index - 1]) starts.push({ block: index, line: 0, offset: 0 });
  });
  return starts;
}

/** page starts (block indices) from Word's layout. */
export function pagesFromWord(layout: WordLayout, used: number): Pages {
  const breaks = wordPageStarts(layout);
  return { starts: breaks.map((each) => each.block), breaks, lastPageUsed: used };
}

/** the pages to show: Word's when it answered for this document, else the estimate. */
export function layoutPages(provider: LayoutProviderName, boxes: BlockBox[], pageHeight: number, word: WordLayout | null): Pages {
  if ((provider === "word" || provider === "engine") && word && word.blockPages.length === boxes.length) {
    const estimate = paginate(boxes, pageHeight);
    return pagesFromWord(word, estimate.lastPageUsed);
  }
  return paginate(boxes, pageHeight);
}

/** "2 minutes ago", "a moment ago". */
export function sinceLabel(checkedAt: number, now: number): string {
  const seconds = Math.max(0, Math.round((now - checkedAt) / 1000));
  if (seconds < 45) return "a moment ago";
  const minutes = Math.round(seconds / 60);
  if (minutes < 60) return minutes === 1 ? "1 minute ago" : `${minutes} minutes ago`;
  const hours = Math.round(minutes / 60);
  return hours === 1 ? "1 hour ago" : `${hours} hours ago`;
}

export interface LayoutCheckState {
  checking: boolean;
  checkedAt: number | null;
  wordInstalled: boolean;
  /** edits made since the last check */
  stale?: boolean;
  /** the last check failed, in a sentence */
  failed?: string | null;
}

/** the status bar's words for where the page breaks come from. */
export function providerLabel(provider: LayoutProviderName, state: LayoutCheckState, now: number): string {
  if (provider === "word") {
    if (state.checking) return "Checking layout with Word…";
    if (state.failed) return "Word couldn’t check the layout";
    if (state.checkedAt === null) return "Layout not checked with Word yet";
    const when = sinceLabel(state.checkedAt, now);
    return state.stale ? `Layout checked with Word ${when}, before your last edits` : `Layout checked with Word ${when}`;
  }
  if (provider === "engine") return "Layout from Letters’ engine";
  return "Pages estimated";
}

/** the plain sentence the indicator's tooltip and Explain give. */
export function providerExplanation(provider: LayoutProviderName, wordInstalled: boolean): string {
  if (provider === "word")
    return "Microsoft Word lays out this document in the background, so lines and pages break exactly where Word breaks them. A check takes about half a minute the first time and is repeated after you pause for a few seconds; Check Now runs it at once.";
  if (provider === "engine") return "Letters’ own layout engine works out the pages. It follows Word’s rules but isn’t Word itself.";
  return wordInstalled
    ? "Page breaks are estimated from the text on screen. Turn on Strict Word mode to have Word lay out the pages."
    : "Page breaks are estimated because Microsoft Word isn’t installed on this computer.";
}

/** how long to wait after the last edit before asking Word again: long enough not to run on every pause. */
export const WORD_IDLE_MS = 4000;

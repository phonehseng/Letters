// where the page breaks on the editing surface come from. three providers,
// best first:
// 1. "word": Microsoft Word lays the document out itself (Strict Word mode,
//    when Word is installed), so pages break exactly where Word breaks them
// 2. "engine": Letters' own layout engine (core/layout), when it lands
// 3. "estimate": measuring the text on screen (pagination.ts)
// swapping in the engine later is one more provider here, nothing else.

import { paginate, type BlockBox, type Pages } from "./pagination";

export type LayoutProviderName = "word" | "engine" | "estimate";

export interface ProviderAvailability {
  /** Microsoft Word is installed (asked once from the shell) */
  word: boolean;
  /** the layout engine exists (core/layout); false until it lands */
  engine: boolean;
}

export type EditingMode = "free" | "strict";

/** the provider to use: Word only in Strict Word mode, then the engine, then the estimate. */
export function chooseProvider(mode: EditingMode, available: ProviderAvailability): LayoutProviderName {
  if (mode === "strict" && available.word) return "word";
  if (available.engine) return "engine";
  return "estimate";
}

/** Word's answer: the page each top-level block starts on (1 = first page), and the total. */
export interface WordLayout {
  blockPages: number[];
  pageCount: number;
}

/** page starts (block indices) from Word's page per block. */
export function pagesFromWord(layout: WordLayout, used: number): Pages {
  const starts = [0];
  layout.blockPages.forEach((page, index) => {
    if (index > 0 && page > layout.blockPages[index - 1]) starts.push(index);
  });
  return { starts, lastPageUsed: used };
}

/** the pages to show: Word's when it answered for this document, else the estimate. */
export function layoutPages(provider: LayoutProviderName, boxes: BlockBox[], pageHeight: number, word: WordLayout | null): Pages {
  if (provider === "word" && word && word.blockPages.length === boxes.length) {
    const estimate = paginate(boxes, pageHeight);
    return pagesFromWord(word, estimate.lastPageUsed);
  }
  return paginate(boxes, pageHeight);
}

/** the status bar's words for where the page breaks come from. */
export function providerLabel(provider: LayoutProviderName, state: { checking: boolean; checkedAt: number | null; wordInstalled: boolean }, now: number): string {
  if (provider === "word") {
    if (state.checking) return "Checking layout with Word…";
    if (state.checkedAt === null) return "Layout from Word";
    const seconds = Math.round((now - state.checkedAt) / 1000);
    return seconds < 45 ? "Layout checked with Word a moment ago" : `Layout checked with Word ${Math.round(seconds / 60)} min ago`;
  }
  if (provider === "engine") return "Layout from Letters’ engine";
  return "Pages estimated";
}

/** the plain sentence the indicator's tooltip and Explain give. */
export function providerExplanation(provider: LayoutProviderName, wordInstalled: boolean): string {
  if (provider === "word") return "Microsoft Word lays out this document in the background, so pages break exactly where Word breaks them. The layout is Word’s own, not an imitation.";
  if (provider === "engine") return "Letters’ own layout engine works out the pages.";
  return wordInstalled
    ? "Page breaks are estimated from the text on screen. Turn on Strict Word mode to have Word lay out the pages."
    : "Page breaks are estimated because Microsoft Word isn’t installed on this computer.";
}

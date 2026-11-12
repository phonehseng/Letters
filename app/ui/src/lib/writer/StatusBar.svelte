<script lang="ts">
  // the strip along the bottom of the writer: words (of the selection when
  // text is selected), about how many pages, the language, and the four
  // typing switches, which are also in the Document tab's Typing section.
  import { tooltipFor } from "../tools";
  import { wordLabel } from "./counts";
  import TypingToggles from "./TypingToggles.svelte";
  import type { WriterSettings } from "./writerApi";

  interface Props {
    words: number;
    selected: number | null;
    page: number;
    pages: number;
    language: string;
    settings: WriterSettings;
    tracking: boolean;
    onSettings: (settings: WriterSettings) => void;
    onTracking: () => void;
    /** "free" or "strict" (Strict Word mode) */
    mode: "free" | "strict";
    onMode: (mode: "free" | "strict") => void;
    /** where page breaks come from, in a few words (layoutSource.ts) */
    layoutLabel: string;
    layoutTooltip: string;
    /** in Strict Word mode: "Everything here opens the same in Word." or what differs */
    compatibility: string | null;
    compatibilityDetails: string;
    /** fonts the document uses that this computer lacks */
    missingFonts?: string[];
  }

  let { words, selected, page, pages, language, settings, tracking, onSettings, onTracking, mode, onMode, layoutLabel, layoutTooltip, compatibility, compatibilityDetails, missingFonts = [] }: Props = $props();

  function languageName(tag: string): string {
    try {
      return new Intl.DisplayNames([navigator.language || "en"], { type: "language" }).of(tag) ?? tag;
    } catch {
      return tag;
    }
  }
</script>

<footer class="glass status" aria-label="Document status">
  <span class="item" title={tooltipFor("wStatusWords")} aria-live="polite">{wordLabel(words, selected)}</span>
  <span class="item" title={tooltipFor("wStatusPages")}>Page {page} of about {pages}</span>
  <span class="item" title={tooltipFor("wStatusLanguage")}>{languageName(language)}</span>
  <span class="item" title={layoutTooltip} aria-live="polite">{layoutLabel}</span>
  <button class="mode" class:strict={mode === "strict"} aria-pressed={mode === "strict"} title={tooltipFor("wStrictMode")} onclick={() => onMode(mode === "strict" ? "free" : "strict")}>
    {mode === "strict" ? "Strict Word" : "Free"}
  </button>
  {#if compatibility}
    <span class="item" title={compatibilityDetails || compatibility}>{compatibility}</span>
  {/if}
  {#if missingFonts.length}
    <span class="item" title={`Not on this computer: ${missingFonts.join(", ")}. Lines use a font with the same widths where one is known.`}>{missingFonts.length} font{missingFonts.length === 1 ? "" : "s"} missing</span>
  {/if}
  {#if tracking}
    <button class="tracking" title={tooltipFor("wTrackChanges")} onclick={onTracking}>Tracking Changes</button>
  {/if}
  <span class="spacer"></span>
  <TypingToggles {settings} compact onChange={onSettings} />
</footer>

<style>
  .status {
    position: absolute;
    left: 0.75rem;
    right: 0.75rem;
    bottom: 0.5rem;
    z-index: 2;
    display: flex;
    align-items: center;
    gap: 1rem;
    min-height: 1.75rem;
    padding: 0.125rem 0.375rem 0.125rem 0.75rem;
    border-radius: 0.875rem;
    color: var(--label-secondary);
    font: var(--footnote);
    letter-spacing: var(--footnote-spacing);
    white-space: nowrap;
    overflow: hidden;
  }

  .spacer {
    flex: 1;
  }

  .mode {
    padding: 0.0625rem 0.5rem;
    border: 0;
    border-radius: 0.625rem;
    background: var(--control);
    color: var(--label-secondary);
    font: var(--footnote);
  }

  .mode.strict {
    background: var(--accent-soft);
    color: var(--accent-label);
  }

  .tracking {
    padding: 0.0625rem 0.5rem;
    border: 0;
    border-radius: 0.625rem;
    background: var(--accent-soft);
    color: var(--accent-label);
    font: var(--footnote);
  }

  @media (max-width: 40rem) {
    .item:nth-child(3) {
      display: none;
    }
  }
</style>

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
    /** asks Word to lay out the pages now (Strict Word mode); null when it can't */
    onCheckLayout?: (() => void) | null;
    /** opens the list of what Word would show differently */
    onShowCompatibility?: () => void;
    /** opens the font library's Missing Fonts view */
    onShowMissingFonts?: (() => void) | null;
    /** the goals line: words of the goal, this session's words and time, the streak (writingGoals.svelte.ts) */
    goals?: string;
  }

  let { words, selected, page, pages, language, settings, tracking, onSettings, onTracking, mode, onMode, layoutLabel, layoutTooltip, compatibility, compatibilityDetails, missingFonts = [], onCheckLayout = null, onShowCompatibility, onShowMissingFonts = null, goals = "" }: Props = $props();

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
  <span class="item" title={tooltipFor("wStatusPages")}>Page {page} of {mode === "strict" && layoutLabel.startsWith("Layout checked") ? "" : "about "}{pages}</span>
  <span class="item" title={tooltipFor("wStatusLanguage")}>{languageName(language)}</span>
  <span class="item" title={layoutTooltip} aria-live="polite">{layoutLabel}</span>
  {#if onCheckLayout}
    <button class="link" title={tooltipFor("wCheckLayout")} onclick={onCheckLayout}>Check Now</button>
  {/if}
  <button class="mode" class:strict={mode === "strict"} aria-pressed={mode === "strict"} title={tooltipFor("wStrictMode")} onclick={() => onMode(mode === "strict" ? "free" : "strict")}>
    {mode === "strict" ? "Strict Word" : "Free"}
  </button>
  {#if compatibility}
    <span class="item" title={compatibilityDetails || compatibility}>{compatibility}</span>
    {#if onShowCompatibility}
      <button class="link" title={tooltipFor("wCompatibility")} onclick={onShowCompatibility}>Show</button>
    {/if}
  {/if}
  {#if missingFonts.length}
    {#if onShowMissingFonts}
      <button class="link" title={`Not on this computer: ${missingFonts.join(", ")}. Click to find them or a font with the same widths.`} onclick={onShowMissingFonts}>{missingFonts.length} font{missingFonts.length === 1 ? "" : "s"} missing</button>
    {:else}
      <span class="item" title={`Not on this computer: ${missingFonts.join(", ")}. Lines use a font with the same widths where one is known.`}>{missingFonts.length} font{missingFonts.length === 1 ? "" : "s"} missing</span>
    {/if}
  {/if}
  {#if tracking}
    <button class="tracking" title={tooltipFor("wTrackChanges")} onclick={onTracking}>Tracking Changes</button>
  {/if}
  {#if goals}
    <span class="item" title="Words toward this document’s goal, this session’s words and time, and the days in a row the daily goal was reached">{goals}</span>
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
    min-height: 2rem;
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

  /* every control in the bar is 24px tall at least (WCAG's target size) */
  .mode,
  .link,
  .tracking {
    min-height: 1.5rem;
  }

  .mode {
    padding: 0.125rem 0.625rem;
    border: 0;
    border-radius: 0.75rem;
    background: var(--control);
    color: var(--label);
    font: var(--footnote);
  }

  /* the label colour, not the accent's, on the soft accent: the accent label
     fell under 4.5:1 there in three of the four modes */
  .mode.strict {
    background: var(--accent-soft);
    color: var(--label);
    box-shadow: inset 0 0 0 1px var(--accent);
  }

  .link {
    padding: 0;
    border: 0;
    background: transparent;
    color: var(--accent-label);
    font: var(--footnote);
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

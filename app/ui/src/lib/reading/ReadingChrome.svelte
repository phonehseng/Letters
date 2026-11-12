<script lang="ts">
  // the reading aids over a pdf: the progress line at the bottom, the page
  // scrubber along the right edge (for long documents), the Go To capsule,
  // and the list of every Find match. the viewer adds this one component.
  import type { SearchMatch } from "../api";
  import { tools, tooltipFor } from "../tools";
  import GoToCapsule from "./GoToCapsule.svelte";
  import type { JumpCandidate } from "./jump";
  import { progressSentence } from "./progress";
  import type { ReadingSession } from "./readingSession.svelte";

  interface Props {
    session: ReadingSession;
    currentPage: number;
    pageCount: number;
    top: number;
    /** the Find bar's matches, for the list of all of them */
    matches: SearchMatch[];
    currentMatch: number;
    onPickMatch: (index: number) => void;
  }

  let { session, currentPage, pageCount, top, matches, currentMatch, onPickMatch }: Props = $props();

  /** the scrubber is worth its space only on long documents */
  const SCRUBBER_FROM_PAGES = 30;

  let showMatches = $state(false);
  let scrubbing = $state<number | null>(null);
  let track = $state<HTMLDivElement>();

  const progress = $derived(progressSentence(currentPage, pageCount, session.secondsPerPage, session.labels[currentPage]));
  const shownPage = $derived(scrubbing ?? currentPage);

  /** the outline entry a page sits under, for the scrubber's label */
  function headingFor(page: number): string {
    let found = "";
    for (const entry of session.outline) if (entry.page <= page) found = entry.title;
    return found;
  }

  function pageAt(event: PointerEvent): number {
    const box = track!.getBoundingClientRect();
    const share = Math.min(1, Math.max(0, (event.clientY - box.top) / box.height));
    return Math.min(pageCount - 1, Math.floor(share * pageCount));
  }

  function startScrub(event: PointerEvent) {
    track?.setPointerCapture(event.pointerId);
    scrubbing = pageAt(event);
  }

  function moveScrub(event: PointerEvent) {
    if (scrubbing === null) return;
    scrubbing = pageAt(event);
  }

  function endScrub() {
    if (scrubbing !== null) session.jump(scrubbing);
    scrubbing = null;
  }

  function onScrubKey(event: KeyboardEvent) {
    const steps: Record<string, number> = { ArrowDown: 1, ArrowUp: -1, PageDown: 10, PageUp: -10 };
    const step = steps[event.key];
    if (step === undefined) return;
    event.preventDefault();
    session.jump(Math.max(0, Math.min(pageCount - 1, currentPage + step)));
  }

  function jumpTo(candidate: JumpCandidate) {
    session.goToOpen = false;
    session.jump(candidate.page);
  }

  function removeBookmarkAt(page: number, label: string) {
    const bookmark = session.bookmarks.find((b) => b.page === page && b.name === label);
    if (bookmark) session.removeBookmark(bookmark.id);
  }
</script>

{#if session.goToOpen}
  <GoToCapsule
    document={session.jumpDocument()}
    {top}
    onJump={jumpTo}
    onAddBookmark={() => session.addBookmark()}
    onRemoveBookmark={removeBookmarkAt}
    onClose={() => (session.goToOpen = false)}
  />
{/if}

<div class="glass progress" title={tooltipFor("readingProgress")}>
  <span aria-live="off">{progress}</span>
  {#if matches.length > 1}
    <button class="link" aria-pressed={showMatches} title={tooltipFor("findResultsList")} onclick={() => (showMatches = !showMatches)}>{tools.findResultsList.label}</button>
  {/if}
</div>

{#if pageCount >= SCRUBBER_FROM_PAGES}
  <div
    class="scrubber"
    bind:this={track}
    role="slider"
    tabindex="0"
    aria-label={tools.scrubber.label}
    aria-valuemin={1}
    aria-valuemax={pageCount}
    aria-valuenow={shownPage + 1}
    aria-valuetext={`Page ${shownPage + 1} of ${pageCount}`}
    title={tooltipFor("scrubber")}
    style:top="{top + 8}px"
    onpointerdown={startScrub}
    onpointermove={moveScrub}
    onpointerup={endScrub}
    onpointercancel={() => (scrubbing = null)}
    onkeydown={onScrubKey}
  >
    <span class="thumb" style:top="{(shownPage / Math.max(1, pageCount - 1)) * 100}%"></span>
    {#if scrubbing !== null}
      <span class="glass bubble" style:top="{(scrubbing / Math.max(1, pageCount - 1)) * 100}%">
        <strong>Page {scrubbing + 1}</strong>
        {#if headingFor(scrubbing)}<span>{headingFor(scrubbing)}</span>{/if}
      </span>
    {/if}
  </div>
{/if}

{#if showMatches && matches.length > 1}
  <section class="glass matches" style:top="{top + 48}px" aria-label="All matches">
    <h2>{matches.length} Matches</h2>
    <ul>
      {#each matches as match, i (i)}
        <li>
          <button class:current={i === currentMatch} onclick={() => onPickMatch(i)}>
            <span class="page">Page {match.page + 1}</span>
            <span class="snippet">{match.snippet}</span>
          </button>
        </li>
      {/each}
    </ul>
  </section>
{/if}

<style>
  .progress {
    position: absolute;
    bottom: 0.75rem;
    left: 50%;
    z-index: 1;
    display: flex;
    gap: 0.75rem;
    align-items: center;
    padding: 0.25rem 0.75rem;
    border-radius: 0.875rem;
    color: var(--label-secondary);
    font: var(--footnote);
    font-variant-numeric: tabular-nums;
    transform: translateX(-50%);
    white-space: nowrap;
  }

  .link {
    border: 0;
    background: transparent;
    color: var(--accent-label);
    font: inherit;
  }

  .scrubber {
    position: absolute;
    right: 0.125rem;
    bottom: 3rem;
    z-index: 1;
    width: 0.875rem;
    border-radius: 0.4375rem;
    cursor: ns-resize;
    touch-action: none;
  }

  .scrubber:hover,
  .scrubber:focus-visible {
    background: var(--control);
  }

  .thumb {
    position: absolute;
    left: 0.1875rem;
    width: 0.5rem;
    height: 1.5rem;
    margin-top: -0.75rem;
    border-radius: 0.25rem;
    background: var(--label-tertiary);
  }

  .bubble {
    position: absolute;
    right: 1.25rem;
    display: flex;
    flex-direction: column;
    min-width: 8rem;
    max-width: 16rem;
    margin-top: -1.25rem;
    padding: 0.375rem 0.625rem;
    border-radius: 0.625rem;
    font: var(--subheadline);
    white-space: nowrap;
  }

  .bubble span {
    overflow: hidden;
    color: var(--label-secondary);
    text-overflow: ellipsis;
  }

  .matches {
    position: absolute;
    right: 1.5rem;
    bottom: 3rem;
    z-index: 2;
    display: flex;
    flex-direction: column;
    width: min(20rem, calc(100% - 3rem));
    padding: 0.75rem;
    border-radius: var(--radius-group);
  }

  .matches h2 {
    margin: 0 0 0.5rem;
    font: var(--headline);
  }

  .matches ul {
    margin: 0;
    padding: 0;
    overflow-y: auto;
    list-style: none;
  }

  .matches button {
    display: flex;
    width: 100%;
    flex-direction: column;
    padding: 0.375rem 0.5rem;
    border: 0;
    border-radius: var(--radius-small);
    background: transparent;
    color: var(--label);
    text-align: left;
    font: var(--subheadline);
  }

  .matches button.current {
    background: var(--accent-soft);
  }

  .page {
    color: var(--label-secondary);
  }
</style>

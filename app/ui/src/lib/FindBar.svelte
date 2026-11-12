<script lang="ts">
  // the find bar: a small capsule under the toolbar's trailing side.
  //
  // it owns the query and which match is current, and tells the viewer
  // whenever the matches or the current one change, so the viewer can
  // draw highlights and scroll. the waiting and the throwing away of
  // stale answers is done by createSearchRunner in search.ts.
  import { onMount, untrack } from "svelte";
  import { searchText, type SearchMatch, type SearchResult } from "./api";
  import Icon from "./Icon.svelte";
  import { createSearchRunner, firstMatchFrom, matchCountLabel, nextMatch, previousMatch } from "./search";
  import { tools, tooltipFor } from "./tools";

  interface Props {
    documentId: number;
    /** the page being read, counted from 0: the search starts there */
    currentPage: number;
    /** distance from the top of the document area, in css pixels */
    top: number;
    /** changes after every edit: the matches then have to be found again */
    revision: number;
    onChange: (matches: SearchMatch[], current: number) => void;
    onClose: () => void;
    /** Find a Topic…, which finds passages by subject on this computer (lib/ai) */
    onFindTopic?: (query: string) => void;
    /** search all documents: a result opens with its words already found (lib/library) */
    initialQuery?: string;
  }

  let { documentId, currentPage, top, revision, onChange, onClose, onFindTopic, initialQuery = "" }: Props = $props();

  let input = $state<HTMLInputElement>();
  let query = $state(untrack(() => initialQuery));
  let result = $state<SearchResult | null>(null);
  let current = $state(-1);
  let failed = $state(false);

  const count = $derived(result?.matches.length ?? 0);
  const label = $derived(failed ? "Search failed" : matchCountLabel(current, result));

  const runner = createSearchRunner({
    search: (text) => searchText(documentId, text),
    onResult: (_text, found) => {
      failed = false;
      result = found;
      showMatch(found ? firstMatchFrom(found.matches, currentPage) : -1);
    },
    onError: () => {
      failed = true;
      result = null;
      showMatch(-1);
    },
  });

  function showMatch(index: number) {
    current = index;
    onChange(result?.matches ?? [], current);
  }

  /** puts the cursor in the field with the old query selected, ready to type over. */
  export function focus() {
    input?.focus();
    input?.select();
  }

  function onKeyDown(event: KeyboardEvent) {
    if (event.key === "Enter") {
      event.preventDefault();
      showMatch(event.shiftKey ? previousMatch(current, count) : nextMatch(current, count));
    } else if (event.key === "Escape") {
      event.preventDefault();
      event.stopPropagation();
      onClose();
    }
  }

  onMount(() => {
    focus();
    // search all documents: the words from the result are found at once
    if (query) runner.update(query);
    return () => runner.cancel();
  });

  // after an edit the words may sit somewhere else on the page (a turned
  // page puts them sideways), so the same query is searched again
  let searchedRevision = untrack(() => revision);
  $effect(() => {
    if (revision === searchedRevision) return;
    searchedRevision = revision;
    untrack(() => runner.update(query));
  });
</script>

<div class="glass find" role="search" style:top="{top}px">
  <Icon name="search" size={0.9375} />
  <input
    type="text"
    placeholder="Find"
    aria-label="Find in this document"
    title={tooltipFor("find")}
    spellcheck="false"
    autocomplete="off"
    bind:this={input}
    bind:value={query}
    oninput={() => runner.update(query)}
    onkeydown={onKeyDown}
  />
  <span class="count" aria-live="polite">{label}</span>
  <button
    class="step"
    title={tooltipFor("findPrevious")}
    aria-label={tools.findPrevious.label}
    disabled={count === 0}
    onclick={() => showMatch(previousMatch(current, count))}
  >
    <Icon name="chevronUp" size={1} />
  </button>
  <button
    class="step"
    title={tooltipFor("findNext")}
    aria-label={tools.findNext.label}
    disabled={count === 0}
    onclick={() => showMatch(nextMatch(current, count))}
  >
    <Icon name="chevronDown" size={1} />
  </button>
  {#if onFindTopic}
    <button class="step topic" title={tooltipFor("findTopic")} aria-label={tools.findTopic.label} onclick={() => onFindTopic(query)}>Topic…</button>
  {/if}
  <button class="step" title="Close Find (Esc)" aria-label="Close Find" onclick={onClose}>
    <Icon name="close" size={0.75} />
  </button>
</div>

<style>
  /* a capsule, the shape of a control that stands on its own */
  .find {
    position: absolute;
    right: 0.75rem;
    z-index: 1;
    display: flex;
    align-items: center;
    gap: 0.25rem;
    width: min(22rem, calc(100% - 1.5rem));
    height: 2.25rem;
    padding: 0 0.25rem 0 0.75rem;
    border-radius: 1.125rem;
    color: var(--label-secondary);
    animation: arrive 0.18s var(--ease);
  }

  .find:focus-within {
    box-shadow:
      0 0 0 1px var(--accent),
      0 0 0 4px var(--accent-soft),
      var(--shadow-glass);
  }

  input {
    flex: 1;
    min-width: 0;
    margin-left: 0.125rem;
    border: 0;
    outline: 0;
    box-shadow: none;
    background: transparent;
    color: var(--label);
    font: var(--body);
  }

  .count {
    color: var(--label-secondary);
    font: var(--subheadline);
    font-variant-numeric: tabular-nums;
    letter-spacing: var(--subheadline-spacing);
    white-space: nowrap;
  }

  /* small round buttons: their inner corner nests inside the capsule */
  .step {
    display: grid;
    place-items: center;
    width: 1.75rem;
    height: 1.75rem;
    border: 0;
    border-radius: 50%;
    background: transparent;
    color: var(--label);
    transition: background 0.15s var(--ease);
  }

  .step:hover:not(:disabled) {
    background: var(--control-hover);
  }

  .step:active:not(:disabled) {
    background: var(--control-active);
  }

  .step:disabled {
    color: var(--label-tertiary);
  }

  /* find a topic: a word, not a symbol, so it reads as another way to find */
  .step.topic {
    width: auto;
    padding: 0 0.5rem;
    border-radius: 0.875rem;
    color: var(--accent-label);
    font: var(--subheadline);
    white-space: nowrap;
  }

  /* a short slide down from the toolbar shows where the bar came from */
  @keyframes arrive {
    from {
      opacity: 0;
      transform: translateY(-0.375rem);
    }
  }
</style>

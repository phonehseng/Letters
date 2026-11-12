<script lang="ts">
  // find and replace: the viewer's find capsule, with a Replace row that
  // shows when asked (Ctrl H or the chevron). Replace changes the current
  // match and moves on; Replace All changes every match in one undo step.
  import { onMount, untrack } from "svelte";
  import Icon from "../Icon.svelte";
  import { tools, tooltipFor } from "../tools";
  import { matchFrom, matchLabel, type Match } from "./find";
  import type { WriterSession } from "./writerSession.svelte";

  interface Props {
    session: WriterSession;
    top: number;
    replaceShown: boolean;
    onClose: () => void;
  }

  let { session, top, replaceShown = $bindable(), onClose }: Props = $props();

  let input = $state<HTMLInputElement>();
  let query = $state("");
  let replacement = $state("");
  let matchCase = $state(false);
  let matches = $state<Match[]>([]);
  let current = $state(-1);

  function search(fromCursor = true) {
    const found = session.find(query, { matchCase }, -1);
    matches = found;
    current = found.length ? (fromCursor ? matchFrom(found, session.editor.selection.from) : 0) : -1;
    if (current >= 0) session.showMatch(found, current);
  }

  function step(direction: 1 | -1) {
    if (matches.length === 0) return;
    current = (current + direction + matches.length) % matches.length;
    session.showMatch(matches, current);
  }

  function replaceOne() {
    const match = matches[current];
    if (!match) return;
    session.replace(match, replacement);
    search();
  }

  function replaceEvery() {
    session.replaceAll(query, replacement, { matchCase });
    search(false);
  }

  export function focus(withReplace = false) {
    if (withReplace) replaceShown = true;
    input?.focus();
    input?.select();
  }

  function onKey(event: KeyboardEvent) {
    if (event.key === "Enter") {
      event.preventDefault();
      step(event.shiftKey ? -1 : 1);
    } else if (event.key === "Escape") {
      event.preventDefault();
      event.stopPropagation();
      onClose();
    }
  }

  onMount(() => {
    focus(replaceShown);
    return () => session.clearFind();
  });

  // the document changed under the matches: find them again. finding sets
  // highlights, which makes a new editor state with the same document, so
  // only a new document counts
  let searchedDoc: unknown = null;
  $effect(() => {
    const doc = session.editor.doc;
    if (doc === searchedDoc) return;
    searchedDoc = doc;
    untrack(() => {
      if (query) {
        matches = session.find(query, { matchCase }, current);
        if (current >= matches.length) current = matches.length - 1;
      }
    });
  });
</script>

<div class="glass find" role="search" aria-label="Find and replace" style:top="{top}px">
  <div class="row">
    <button class="step" title="Show Replace" aria-label="Show Replace" aria-expanded={replaceShown} onclick={() => (replaceShown = !replaceShown)}>
      <Icon name={replaceShown ? "chevronDown" : "chevronRight"} size={0.875} />
    </button>
    <input
      type="text"
      placeholder="Find"
      aria-label="Find in this document"
      title={tooltipFor("find")}
      spellcheck="false"
      bind:this={input}
      bind:value={query}
      oninput={() => search()}
      onkeydown={onKey}
    />
    <label class="case" title="Match capital letters exactly">
      <input type="checkbox" bind:checked={matchCase} onchange={() => search()} />Aa
    </label>
    <span class="count" aria-live="polite">{query ? matchLabel(current, matches.length) : ""}</span>
    <button class="step" title={tooltipFor("findPrevious")} aria-label={tools.findPrevious.label} disabled={!matches.length} onclick={() => step(-1)}>
      <Icon name="chevronUp" size={1} />
    </button>
    <button class="step" title={tooltipFor("findNext")} aria-label={tools.findNext.label} disabled={!matches.length} onclick={() => step(1)}>
      <Icon name="chevronDown" size={1} />
    </button>
    <button class="step" title="Close Find (Esc)" aria-label="Close Find" onclick={onClose}>
      <Icon name="close" size={0.75} />
    </button>
  </div>
  {#if replaceShown}
    <div class="row">
      <input
        class="replace-field"
        type="text"
        placeholder="Replace with"
        aria-label="Replace with"
        bind:value={replacement}
        onkeydown={(event) => {
          if (event.key === "Enter") {
            event.preventDefault();
            replaceOne();
          } else onKey(event);
        }}
      />
      <button class="button" title={tooltipFor("wReplace")} disabled={!matches.length} onclick={replaceOne}>{tools.wReplace.label}</button>
      <button class="button" title={tooltipFor("wReplaceAll")} disabled={!matches.length} onclick={replaceEvery}>{tools.wReplaceAll.label}</button>
    </div>
  {/if}
</div>

<style>
  .find {
    position: absolute;
    right: 0.75rem;
    z-index: 3;
    display: flex;
    flex-direction: column;
    gap: 0.25rem;
    width: min(26rem, calc(100% - 1.5rem));
    padding: 0.25rem 0.25rem 0.25rem 0.375rem;
    border-radius: 1.125rem;
    color: var(--label-secondary);
    animation: arrive 0.18s var(--ease);
  }

  .find:focus-within {
    box-shadow: 0 0 0 1px var(--accent), 0 0 0 4px var(--accent-soft), var(--shadow-glass);
  }

  .row {
    display: flex;
    align-items: center;
    gap: 0.25rem;
    min-height: 1.75rem;
  }

  input[type="text"] {
    flex: 1;
    min-width: 0;
    border: 0;
    outline: 0;
    box-shadow: none;
    background: transparent;
    color: var(--label);
    font: var(--body);
  }

  .replace-field {
    margin-left: 2rem;
    padding: 0.25rem 0;
  }

  .case {
    display: flex;
    align-items: center;
    gap: 0.125rem;
    font: var(--subheadline);
  }

  .count {
    font: var(--subheadline);
    font-variant-numeric: tabular-nums;
    white-space: nowrap;
  }

  .step {
    display: grid;
    place-items: center;
    width: 1.75rem;
    height: 1.75rem;
    border: 0;
    border-radius: 50%;
    background: transparent;
    color: var(--label);
  }

  .step:hover:not(:disabled) {
    background: var(--control-hover);
  }

  .step:disabled {
    color: var(--label-tertiary);
  }

  @keyframes arrive {
    from {
      opacity: 0;
      transform: translateY(-0.375rem);
    }
  }
</style>

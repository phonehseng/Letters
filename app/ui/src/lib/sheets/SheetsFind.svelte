<script lang="ts">
  // find and replace, a capsule under the toolbar as in the other
  // screens. Enter goes to the next match (Shift Enter the one before);
  // * and ? work as in excel. Replace changes the match on screen,
  // Replace All every match in one undo step.
  import { findCells, type FindOptions, type Found } from "./sheetsApi";
  import { cellName } from "./geometry";

  interface Props {
    id: number;
    sheet: number;
    sheetNames: string[];
    showReplace: boolean;
    onGo: (found: Found) => void;
    onReplaceOne: (found: Found, find: string, replacement: string, options: FindOptions) => void;
    onReplaceAll: (find: string, replacement: string, options: FindOptions, sheet: number | null) => void;
    onClose: () => void;
  }

  let { id, sheet, sheetNames, showReplace = $bindable(), onGo, onReplaceOne, onReplaceAll, onClose }: Props = $props();
  let text = $state("");
  let replacement = $state("");
  let options = $state<FindOptions>({ matchCase: false, wholeCell: false, inFormulas: false });
  let everySheet = $state(false);
  let results = $state<Found[] | null>(null);
  let index = $state(-1);
  let searched = "";

  async function search() {
    searched = JSON.stringify([text, options, everySheet, sheet]);
    results = text ? await findCells(id, everySheet ? null : sheet, text, options) : [];
    index = -1;
  }

  async function next(backwards: boolean) {
    if (JSON.stringify([text, options, everySheet, sheet]) !== searched || results === null) await search();
    if (!results || results.length === 0) return;
    index = (index + (backwards ? -1 : 1) + results.length) % results.length;
    onGo(results[index]);
  }

  function replaceOne() {
    if (!results || index < 0) return void next(false);
    onReplaceOne(results[index], text, replacement, options);
    results = null;
  }

  const summary = $derived.by(() => {
    if (results === null) return "";
    if (results.length === 0) return "No matches.";
    const at = index >= 0 ? results[index] : null;
    const where = at ? ` · ${everySheet ? `${sheetNames[at.sheet]}!` : ""}${cellName(at.row, at.col)}` : "";
    return `${index >= 0 ? `${index + 1} of ` : ""}${results.length} ${results.length === 1 ? "match" : "matches"}${where}`;
  });
</script>

<div class="glass find" role="search" aria-label="Find and replace">
  <div class="line">
    <!-- svelte-ignore a11y_autofocus -->
    <input
      class="field"
      placeholder="Find"
      aria-label="Find"
      bind:value={text}
      autofocus
      onkeydown={(event) => {
        if (event.key === "Enter") void next(event.shiftKey);
        if (event.key === "Escape") onClose();
        event.stopPropagation();
      }}
    />
    <button class="button" onclick={() => next(true)} title="Previous match (Shift Enter)">Previous</button>
    <button class="button" onclick={() => next(false)} title="Next match (Enter)">Next</button>
    <span class="summary" aria-live="polite">{summary}</span>
    <button class="button" onclick={() => (showReplace = !showReplace)} aria-pressed={showReplace}>Replace</button>
    <button class="button" onclick={onClose}>Done</button>
  </div>
  <div class="line options">
    <label><input type="checkbox" bind:checked={options.matchCase} /> Match Case</label>
    <label><input type="checkbox" bind:checked={options.wholeCell} /> Whole Cell</label>
    <label><input type="checkbox" bind:checked={options.inFormulas} /> Look in Formulas</label>
    <label><input type="checkbox" bind:checked={everySheet} /> Every Sheet</label>
  </div>
  {#if showReplace}
    <div class="line">
      <input class="field" placeholder="Replace with" aria-label="Replace with" bind:value={replacement} onkeydown={(event) => event.stopPropagation()} />
      <button class="button" onclick={replaceOne}>Replace</button>
      <button class="button" disabled={!text} onclick={() => onReplaceAll(text, replacement, options, everySheet ? null : sheet)}>Replace All</button>
    </div>
  {/if}
</div>

<style>
  .find {
    position: absolute;
    top: 3.75rem;
    right: 1rem;
    z-index: 6;
    display: flex;
    flex-direction: column;
    gap: 0.375rem;
    width: min(38rem, calc(100% - 2rem));
    padding: 0.5rem;
    border-radius: var(--radius-group);
  }

  .line {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 0.375rem;
  }

  .field {
    flex: 1 1 10rem;
  }

  .summary {
    color: var(--label-secondary);
    font: var(--subheadline);
  }

  .options {
    gap: 0.875rem;
    color: var(--label-secondary);
    font: var(--subheadline);
  }
</style>

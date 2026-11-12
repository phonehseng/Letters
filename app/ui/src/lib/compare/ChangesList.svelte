<script lang="ts">
  // the Changes sidebar: the summary line, filters by category, Next and
  // Previous Change, and every change as one plain sentence under its
  // page. only the rows in view are made. the list is one listbox: the
  // arrows move through the changes, and the picked one shows on both sides.
  import { tick } from "svelte";
  import { remInPixels } from "../dom";
  import { tooltipFor } from "../tools";
  import type { Category, ComparisonResult } from "./compareApi";
  import { FILTERS, ignoredLine, kindLabel, markStyle, positionText, summaryLine, type ListRow } from "./changes";
  import { rowTops, scrollToRow, visibleRows } from "./changeList";

  interface Props {
    result: ComparisonResult;
    rows: ListRow[];
    ordered: { pair: number; change: { id: number } }[];
    shown: Set<Category>;
    selectedId: number | null;
    onFilter: (category: Category, on: boolean) => void;
    onSelect: (id: number) => void;
    onStep: (direction: "next" | "previous") => void;
  }

  let { result, rows, ordered, shown, selectedId, onFilter, onSelect, onStep }: Props = $props();

  let list = $state<HTMLDivElement>();
  let scrollTop = $state(0);
  let height = $state(0);
  const remPx = remInPixels();

  const placed = $derived(rowTops(rows, remPx));
  const visible = $derived(visibleRows(rows, placed.tops, remPx, scrollTop, height));
  const selectedRow = $derived(rows.findIndex((row) => row.type === "change" && row.change.id === selectedId));
  const changeIds = $derived(rows.flatMap((row) => (row.type === "change" ? [row.change.id] : [])));

  // the picked change is scrolled into the list's view
  $effect(() => {
    const index = selectedRow;
    if (index < 0 || !list) return;
    const target = scrollToRow(placed.tops[index], (rows[index].type === "heading" ? 2 : 3.5) * remPx, list.scrollTop, list.clientHeight);
    if (target !== null) list.scrollTop = target;
  });

  async function onKeyDown(event: KeyboardEvent) {
    const index = changeIds.indexOf(selectedId ?? -1);
    let next: number | null = null;
    if (event.key === "ArrowDown") next = Math.min(changeIds.length - 1, index + 1);
    else if (event.key === "ArrowUp") next = Math.max(0, index - 1);
    else if (event.key === "Home") next = 0;
    else if (event.key === "End") next = changeIds.length - 1;
    if (next === null || changeIds.length === 0 || event.altKey) return;
    event.preventDefault();
    onSelect(changeIds[Math.max(0, next)]);
    await tick();
  }

  const optionId = (id: number) => `compare-change-${id}`;
</script>

<aside class="changes" aria-label="Changes">
  <header>
    <h2>Changes</h2>
    <p class="summary" aria-live="polite">{summaryLine(result.summary)}</p>
    {#if ignoredLine(result.summary)}
      <p class="hint">{ignoredLine(result.summary)}</p>
    {/if}
    {#each result.notes as note, index (index)}
      <p class="hint">{note}</p>
    {/each}
  </header>

  {#if result.summary.total > 0}
    <fieldset class="filters">
      <legend class="caption">Show</legend>
      {#each FILTERS as filter (filter.category)}
        <label class="check">
          <input type="checkbox" checked={shown.has(filter.category)} onchange={(event) => onFilter(filter.category, event.currentTarget.checked)} />
          <span>{filter.label} <span class="count">{result.summary[filter.category]}</span></span>
        </label>
      {/each}
    </fieldset>

    <div class="steps">
      <button class="button" title={tooltipFor("comparePreviousChange")} disabled={ordered.length === 0} onclick={() => onStep("previous")}>Previous Change</button>
      <button class="button" title={tooltipFor("compareNextChange")} disabled={ordered.length === 0} onclick={() => onStep("next")}>Next Change</button>
    </div>
    <p class="hint position" aria-live="polite">{positionText(ordered, selectedId) || `${ordered.length} shown`}</p>

    <div
      class="list"
      role="listbox"
      tabindex="0"
      aria-label="Changes, grouped by page"
      aria-activedescendant={selectedId !== null && selectedRow >= 0 ? optionId(selectedId) : undefined}
      bind:this={list}
      bind:clientHeight={height}
      onscroll={() => (scrollTop = list?.scrollTop ?? 0)}
      onkeydown={onKeyDown}
    >
      <div class="rows" style:height="{placed.total}px">
        {#each visible as spot (spot.index)}
          {@const row = rows[spot.index]}
          {#if row.type === "heading"}
            <div class="heading" role="presentation" style:top="{spot.top}px" style:height="{spot.height}px">
              {row.text} <span class="count">{row.count}</span>
            </div>
          {:else}
            {@const style = markStyle(row.change.kind, "new")}
            <!-- svelte-ignore a11y_click_events_have_key_events -->
            <div
              id={optionId(row.change.id)}
              class="change"
              class:selected={row.change.id === selectedId}
              role="option"
              tabindex="-1"
              aria-selected={row.change.id === selectedId}
              title={row.change.detail}
              style:top="{spot.top}px"
              style:height="{spot.height}px"
              onclick={() => onSelect(row.change.id)}
            >
              <span class="kind {style.tone}">{kindLabel(row.change.kind)}</span>
              <span class="detail">{row.change.detail}</span>
            </div>
          {/if}
        {/each}
      </div>
    </div>
  {:else}
    <p class="same">{summaryLine(result.summary)}</p>
    <p class="hint">Their words, formatting, layout and pictures match, even if the files themselves differ.</p>
  {/if}
</aside>

<style>
  .changes {
    display: flex;
    flex-direction: column;
    gap: 0.5rem;
    width: 20rem;
    max-width: 45%;
    height: 100%;
    padding: calc(var(--toolbar-space) + 0.25rem) 0.75rem 0.75rem;
    border-left: 0.5px solid var(--separator);
    background: var(--glass);
    backdrop-filter: var(--blur);
  }

  header {
    display: flex;
    flex-direction: column;
    gap: 0.25rem;
  }

  h2 {
    margin: 0;
    font: var(--title);
    letter-spacing: var(--title-spacing);
  }

  .summary {
    margin: 0;
    font-weight: 600;
  }

  .filters {
    display: flex;
    flex-wrap: wrap;
    gap: 0.25rem 0.75rem;
    margin: 0;
    padding: 0;
    border: 0;
  }

  .filters legend {
    width: 100%;
    margin-bottom: 0.25rem;
  }

  .count {
    color: var(--label-secondary);
    font: var(--subheadline);
  }

  .steps {
    display: flex;
    gap: 0.375rem;
  }

  .steps .button {
    flex: 1;
  }

  .position {
    margin: 0;
  }

  .list {
    position: relative;
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    border-radius: var(--radius-small);
    outline: none;
  }

  .list:focus-visible {
    box-shadow: inset 0 0 0 2px var(--accent);
  }

  .rows {
    position: relative;
  }

  .heading,
  .change {
    position: absolute;
    left: 0;
    right: 0;
  }

  .heading {
    display: flex;
    align-items: flex-end;
    gap: 0.375rem;
    padding: 0 0.25rem 0.25rem;
    font: var(--headline);
  }

  .change {
    display: flex;
    flex-direction: column;
    gap: 0.125rem;
    padding: 0.375rem 0.5rem;
    overflow: hidden;
    border-radius: var(--radius-small);
  }

  .change:hover {
    background: var(--control-hover);
  }

  .change.selected {
    background: var(--accent-soft);
  }

  .kind {
    align-self: flex-start;
    padding: 0 0.3125rem;
    border-radius: 0.25rem;
    color: #1d1d1f;
    font: var(--footnote);
    font-weight: 700;
  }

  /* the same tones as the marks on the pages, with dark text on them */
  .removed {
    background: rgb(255 59 48 / 0.3);
  }
  .added {
    background: rgb(52 199 89 / 0.35);
  }
  .moved {
    background: rgb(175 82 222 / 0.3);
  }
  .formatting {
    background: rgb(255 204 0 / 0.45);
  }
  .layout {
    background: rgb(48 176 199 / 0.3);
  }
  .picture {
    background: rgb(255 149 0 / 0.35);
  }

  .detail {
    display: -webkit-box;
    overflow: hidden;
    -webkit-box-orient: vertical;
    -webkit-line-clamp: 2;
    line-clamp: 2;
    line-height: 1rem;
  }

  .same {
    margin: 1rem 0 0;
    font: var(--headline);
  }

  @media (prefers-color-scheme: dark) {
    .kind {
      color: #ffffff;
    }
  }
</style>

<script lang="ts">
  // the glass strip along the bottom: the sheet tabs on the leading side;
  // on the trailing side what excel's status bar shows for a selection
  // (average, count, sum), a circular reference when there is one, and
  // the zoom. a live region says the sums for screen readers.
  import type { Snippet } from "svelte";
  import type { SelectionStats } from "./sheetsApi";
  import { tooltipFor } from "../tools";

  interface Props {
    tabs: Snippet;
    stats: SelectionStats | null;
    circular: string[];
    zoom: number;
    /** "Ready", "Enter", "Edit" or "Point", as excel names its modes */
    mode: string;
    onCircular: (cell: string) => void;
    onZoom: () => void;
  }

  let { tabs, stats, circular, zoom, mode, onCircular, onZoom }: Props = $props();

  const numbers = new Intl.NumberFormat(undefined, { maximumFractionDigits: 10 });
  const shown = $derived(stats && stats.count > 1 ? stats : null);
</script>

<footer class="glass status">
  <div class="leading">{@render tabs()}</div>
  <div class="trailing">
    <span class="mode" aria-hidden="true">{mode}</span>
    {#if circular.length > 0}
      <button class="circular" title="Formulas that depend on their own result show 0. Click to go to one and see why." onclick={() => onCircular(circular[0])}>Circular reference: {circular[0]}</button>
    {/if}
    <span class="numbers" title={tooltipFor("sheetsStatus")} aria-live="polite">
      {#if shown}
        {#if shown.average !== undefined}<span>Average: {numbers.format(shown.average)}</span>{/if}
        <span>Count: {numbers.format(shown.count)}</span>
        {#if shown.numericCount > 0}<span>Sum: {numbers.format(shown.sum)}</span>{/if}
      {/if}
    </span>
    <button class="zoom" title="Zoom (Ctrl Alt = and Ctrl Alt −). Click for 100%." onclick={onZoom}>{Math.round(zoom * 100)}%</button>
  </div>
</footer>

<style>
  .status {
    display: flex;
    align-items: center;
    gap: 1rem;
    margin: 0.375rem 0.5rem 0.5rem;
    padding: 0.25rem;
    border-radius: var(--radius-group);
  }

  .leading {
    flex: 1;
    min-width: 0;
  }

  .trailing {
    display: flex;
    flex: none;
    align-items: center;
    gap: 1rem;
    padding-right: 0.5rem;
    color: var(--label-secondary);
    font: var(--subheadline);
  }

  .numbers {
    display: flex;
    gap: 1rem;
    font-variant-numeric: tabular-nums;
  }

  .circular {
    border: 0;
    background: transparent;
    color: var(--danger);
    font: var(--subheadline);
    text-decoration: underline;
  }

  .zoom {
    min-width: 3rem;
    border: 0;
    border-radius: var(--radius-small);
    background: transparent;
    color: var(--label-secondary);
    font: var(--subheadline);
  }

  .zoom:hover {
    background: var(--control-hover);
  }
</style>

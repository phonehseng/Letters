<script lang="ts">
  // the menu of a filter button: every value the column shows, ticked to
  // keep, with Select All, a search field for long lists, the two sorts
  // and Clear Filter. like excel's, the rows whose value is unticked are
  // hidden, never deleted.
  import { onMount } from "svelte";
  import { columnValues, type Range } from "./sheetsApi";

  interface Props {
    id: number;
    sheet: number;
    range: Range;
    column: number;
    x: number;
    y: number;
    filtered: boolean;
    onApply: (keep: string[]) => void;
    onSort: (descending: boolean) => void;
    onClear: () => void;
    onClose: () => void;
  }

  let { id, sheet, range, column, x, y, filtered, onApply, onSort, onClear, onClose }: Props = $props();
  let values = $state<string[]>([]);
  let kept = $state(new Set<string>());
  let search = $state("");
  let menu = $state<HTMLDivElement>();

  const shown = $derived(values.filter((value) => value.toLowerCase().includes(search.toLowerCase())));

  onMount(async () => {
    values = await columnValues(id, sheet, range, column);
    kept = new Set(values);
    menu?.querySelector("input")?.focus();
  });

  function toggle(value: string) {
    const next = new Set(kept);
    if (next.has(value)) next.delete(value);
    else next.add(value);
    kept = next;
  }
</script>

<svelte:window onpointerdown={(event) => !menu?.contains(event.target as Node) && onClose()} />

<!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
<div class="glass menu" role="dialog" aria-label="Filter" tabindex="-1" bind:this={menu} style:left="{x}px" style:top="{y}px" onkeydown={(event) => event.key === "Escape" && onClose()}>
  <button class="item" onclick={() => onSort(false)}>Sort A to Z</button>
  <button class="item" onclick={() => onSort(true)}>Sort Z to A</button>
  <div class="separator"></div>
  <input class="field" placeholder="Search" aria-label="Search the values" bind:value={search} />
  <label class="value all"><input type="checkbox" checked={kept.size === values.length} onchange={(event) => (kept = new Set(event.currentTarget.checked ? values : []))} /> Select All</label>
  <div class="values">
    {#each shown as value (value)}
      <label class="value"><input type="checkbox" checked={kept.has(value)} onchange={() => toggle(value)} /> {value === "" ? "(Blanks)" : value}</label>
    {/each}
  </div>
  <p class="note">Unticked values’ rows are hidden, not deleted.</p>
  <div class="buttons">
    {#if filtered}<button class="button" onclick={onClear}>Clear Filter</button>{/if}
    <button class="button" onclick={onClose}>Cancel</button>
    <button class="button primary" disabled={kept.size === 0} onclick={() => onApply([...kept])}>Filter</button>
  </div>
</div>

<style>
  .menu {
    position: fixed;
    z-index: 20;
    display: flex;
    flex-direction: column;
    gap: 0.25rem;
    width: 16rem;
    padding: 0.5rem;
    border-radius: var(--radius-group);
  }

  .item {
    padding: 0.25rem 0.5rem;
    border: 0;
    border-radius: 0.25rem;
    background: transparent;
    color: var(--label);
    font: var(--body);
    text-align: left;
  }

  .item:hover {
    background: var(--control-hover);
  }

  .separator {
    height: 1px;
    margin: 0.25rem 0;
    background: var(--separator);
  }

  .values {
    max-height: 14rem;
    overflow: auto;
  }

  .value {
    display: flex;
    gap: 0.375rem;
    align-items: center;
    padding: 0.125rem 0.25rem;
    font: var(--body);
  }

  .note {
    margin: 0.25rem 0;
    color: var(--label-secondary);
    font: var(--footnote);
  }

  .buttons {
    display: flex;
    justify-content: flex-end;
    gap: 0.375rem;
  }
</style>

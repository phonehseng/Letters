<script lang="ts">
  // the document's outline: its chapters and sections, indented by level.
  // the part being read is highlighted, and a click goes to its page.
  //
  // it is a tree for screen readers ("level 2, 3 of 5"), moved through
  // with the arrow keys. only one entry is a tab stop at a time.
  import { tick, untrack } from "svelte";
  import type { OutlineItem } from "./api";
  import { nextIndex } from "./keyboard";
  import { currentOutlineIndex, treePositions } from "./outline";
  import { tools } from "./tools";

  interface Props {
    /** null while the outline is still being read */
    items: OutlineItem[] | null;
    /** the page being read, counted from 0 */
    currentPage: number;
    onGoToPage: (index: number) => void;
    onShowThumbnails: () => void;
  }

  let { items, currentPage, onGoToPage, onShowThumbnails }: Props = $props();

  let entries: HTMLDivElement[] = $state([]);
  /** the entry that has (or last had) the keyboard focus */
  let focused = $state(-1);

  const positions = $derived(treePositions(items ?? []));
  const current = $derived(currentOutlineIndex(items ?? [], currentPage));
  // the tab stop is the focused entry, else the current section, else the first
  const tabStop = $derived(focused >= 0 ? focused : Math.max(current, 0));

  async function onKeyDown(event: KeyboardEvent, index: number, item: OutlineItem) {
    if (event.key === "Enter" || event.key === " ") {
      event.preventDefault();
      onGoToPage(item.page);
      return;
    }
    const target = nextIndex(index, event.key, entries.length, { orientation: "vertical", wrap: false });
    if (target === null) return;
    event.preventDefault();
    focused = target;
    await tick();
    entries[target]?.focus();
  }

  // keep the current section in view as the reader moves through the document
  $effect(() => {
    const index = current;
    untrack(() => entries[index]?.scrollIntoView({ block: "nearest" }));
  });
</script>

{#if items === null}
  <p class="loading" aria-busy="true">Loading…</p>
{:else if items.length === 0}
  <!-- an empty state always offers the next step -->
  <div class="empty">
    <h3>No Outline</h3>
    <p>This document has no bookmarks. Use Thumbnails to move around.</p>
    <button class="button" onclick={onShowThumbnails}>Show {tools.thumbnails.label}</button>
  </div>
{:else}
  <div class="tree" role="tree" aria-label={tools.outline.label}>
    {#each items as item, index (index)}
      <div
        class="entry"
        class:current={index === current}
        role="treeitem"
        aria-level={positions[index].level}
        aria-posinset={positions[index].positionInSet}
        aria-setsize={positions[index].setSize}
        aria-selected={index === current}
        aria-current={index === current ? "location" : undefined}
        tabindex={index === tabStop ? 0 : -1}
        style:padding-left="{0.5 + Math.min(positions[index].level - 1, 6) * 0.875}rem"
        bind:this={entries[index]}
        onclick={() => onGoToPage(item.page)}
        onkeydown={(event) => onKeyDown(event, index, item)}
        onfocus={() => (focused = index)}
      >
        <span class="title">{item.title}</span>
        <span class="page" aria-label={`page ${item.page + 1}`}>{item.page + 1}</span>
      </div>
    {/each}
  </div>
{/if}

<style>
  .tree {
    display: flex;
    flex-direction: column;
    gap: 0.0625rem;
  }

  .entry {
    display: flex;
    align-items: baseline;
    gap: 0.5rem;
    padding: 0.3125rem 0.5rem;
    border-radius: var(--radius-small);
    font: var(--body);
    transition: background 0.15s var(--ease);
  }

  .entry:hover {
    background: var(--control-hover);
  }

  .entry:active {
    background: var(--control-active);
  }

  /* the part being read: a selection, so it gets the accent */
  .entry.current {
    background: var(--accent-soft);
    color: var(--accent-label);
    font-weight: 600;
  }

  .title {
    flex: 1;
    min-width: 0;
    overflow-wrap: anywhere;
  }

  .page {
    color: var(--label-secondary);
    font: var(--subheadline);
    font-variant-numeric: tabular-nums;
    letter-spacing: var(--subheadline-spacing);
  }

  .loading {
    margin: 1rem 0.5rem;
    color: var(--label-secondary);
  }

  .empty {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 0.5rem;
    padding: 2rem 0.75rem;
    text-align: center;
  }

  .empty h3 {
    margin: 0;
    font: var(--headline);
  }

  .empty p {
    margin: 0 0 0.25rem;
    color: var(--label-secondary);
    line-height: 1.125rem;
  }
</style>

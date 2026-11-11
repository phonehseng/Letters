<script lang="ts">
  // the sidebar on the leading side: page thumbnails or the outline,
  // chosen with a small segmented control at its top (as in Pages).
  //
  // like the explain panel it runs the full height of the window, edge to
  // edge, in the glass material of the floating layer.
  import { tick } from "svelte";
  import type { OutlineItem } from "./api";
  import { nextIndex } from "./keyboard";
  import type { PageSize } from "./layout";
  import Outline from "./Outline.svelte";
  import type { SidebarMode } from "./sidebar";
  import Thumbnails from "./Thumbnails.svelte";
  import { tools } from "./tools";

  interface Props {
    mode: SidebarMode;
    documentId: number;
    pages: PageSize[];
    currentRow: { first: number; last: number };
    pixelRatio: number;
    /** null while the outline is still being read */
    outline: OutlineItem[] | null;
    onModeChange: (mode: SidebarMode) => void;
    onGoToPage: (index: number) => void;
  }

  let { mode, documentId, pages, currentRow, pixelRatio, outline, onModeChange, onGoToPage }: Props = $props();

  const modes: SidebarMode[] = ["thumbnails", "outline"];
  let tabs: HTMLButtonElement[] = $state([]);

  // tabs follow the usual pattern: one tab stop, the arrows switch views
  async function onTabKeyDown(event: KeyboardEvent) {
    const target = nextIndex(modes.indexOf(mode), event.key, modes.length, { orientation: "horizontal", wrap: true });
    if (target === null) return;
    event.preventDefault();
    onModeChange(modes[target]);
    await tick();
    tabs[target]?.focus();
  }
</script>

<aside class="sidebar" aria-label={tools.sidebar.label}>
  <div class="segments" role="tablist" aria-label="Show in the sidebar">
    {#each modes as option, index (option)}
      <button
        id={`sidebar-tab-${option}`}
        class="segment"
        class:selected={mode === option}
        role="tab"
        aria-selected={mode === option}
        aria-controls="sidebar-panel"
        tabindex={mode === option ? 0 : -1}
        title={tools[option].tooltip}
        bind:this={tabs[index]}
        onclick={() => onModeChange(option)}
        onkeydown={onTabKeyDown}
      >
        {tools[option].label}
      </button>
    {/each}
  </div>

  <div id="sidebar-panel" class="panel" role="tabpanel" aria-labelledby={`sidebar-tab-${mode}`}>
    {#if mode === "thumbnails"}
      <Thumbnails {documentId} {pages} {currentRow} {pixelRatio} {onGoToPage} />
    {:else}
      <Outline
        items={outline}
        currentPage={currentRow.first}
        {onGoToPage}
        onShowThumbnails={() => onModeChange("thumbnails")}
      />
    {/if}
  </div>
</aside>

<style>
  .sidebar {
    display: flex;
    flex-direction: column;
    gap: 0.625rem;
    width: 12rem;
    max-width: 40vw;
    height: 100%;
    padding: 0.75rem 0.5rem 0;
    border-right: 0.5px solid var(--separator);
    background: var(--glass);
    backdrop-filter: var(--blur);
  }

  /* a small segmented control. its outer corner is the inner one plus
     the padding, so the selected segment nests inside it */
  .segments {
    display: flex;
    flex-shrink: 0;
    padding: 0.125rem;
    border-radius: calc(var(--radius-small) + 0.125rem);
    background: var(--control);
  }

  .segment {
    flex: 1;
    min-width: 0;
    height: 1.5rem;
    padding: 0 0.375rem;
    overflow: hidden;
    border: 0;
    border-radius: var(--radius-small);
    background: transparent;
    color: var(--label-secondary);
    font: var(--subheadline);
    font-weight: 500;
    letter-spacing: var(--subheadline-spacing);
    text-overflow: ellipsis;
    white-space: nowrap;
    transition: background 0.15s var(--ease);
  }

  .segment:hover:not(.selected) {
    background: var(--control-hover);
  }

  /* the chosen view is raised, not coloured: the accent is saved for the page */
  .segment.selected {
    background: var(--segment-selected);
    box-shadow: var(--shadow-segment);
    color: var(--label);
  }

  .panel {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    padding-bottom: 0.75rem;
  }
</style>

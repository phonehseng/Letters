<script lang="ts">
  // one side of Side by Side: a scrolling column with a slot for every
  // page pair. a slot shows this side's page, drawn with the viewer's
  // tiles, or a quiet placeholder when only the other version has that
  // page. only the slots near the screen exist, so 500 pages scroll as
  // lightly as 5.
  import type { PageLayout } from "../layout";
  import { scaleMilli, tilesForPage, visiblePages } from "../layout";
  import PageView from "../PageView.svelte";
  import ChangeMarks from "./ChangeMarks.svelte";
  import type { Change } from "./compareApi";
  import type { Side } from "./changes";
  import type { Slot } from "./scrollSync";

  interface Props {
    side: Side;
    /** "Old version: essay v1.pdf", for the region's name */
    label: string;
    documentId: number;
    slots: Slot[];
    layout: PageLayout;
    zoom: number;
    pixelRatio: number;
    changesOnPage: Map<number, Change[]>;
    selectedId: number | null;
    /** room at the top for the floating toolbar */
    inset: number;
    onSelect: (id: number) => void;
    onScroll: (scrollTop: number) => void;
    scroller?: HTMLDivElement;
  }

  let {
    side,
    label,
    documentId,
    slots,
    layout,
    zoom,
    pixelRatio,
    changesOnPage,
    selectedId,
    inset,
    onSelect,
    onScroll,
    scroller = $bindable(),
  }: Props = $props();

  let scrollTop = $state(0);
  let viewportHeight = $state(0);
  let viewportWidth = $state(0);

  const scale = $derived(scaleMilli(zoom, pixelRatio));
  const contentWidth = $derived(Math.max(layout.widestRow + 32, viewportWidth));
  const shown = $derived.by(() => {
    const range = visiblePages(layout.pages, scrollTop, viewportHeight, viewportHeight / 2);
    return range ? layout.pages.slice(range.first, range.last + 1) : [];
  });

  function handleScroll() {
    scrollTop = scroller?.scrollTop ?? 0;
    onScroll(scrollTop);
  }

  function tilesOf(slot: Slot, top: number) {
    const margin = viewportHeight / 2;
    return tilesForPage(slot.size, scale, pixelRatio, scrollTop - top - margin, scrollTop + viewportHeight - top + margin);
  }

  const missingText = $derived(side === "old" ? "Not in the old version" : "Not in the new version");
</script>

<!-- svelte-ignore a11y_no_noninteractive_tabindex -->
<div
  class="column"
  role="region"
  aria-label={label}
  tabindex="0"
  bind:this={scroller}
  bind:clientHeight={viewportHeight}
  bind:clientWidth={viewportWidth}
  onscroll={handleScroll}
>
  <div class="content" style:width="{contentWidth}px" style:height="{layout.totalHeight}px" style:--inset="{inset}px">
    {#each shown as placed (placed.index)}
      {@const slot = slots[placed.index]}
      {@const left = contentWidth / 2 + placed.left}
      {#if slot.page !== null}
        <PageView
          {documentId}
          page={{ ...placed, index: slot.page }}
          {left}
          {scale}
          tiles={tilesOf(slot, placed.top)}
          revision={0}
        />
        <div
          class="marks-layer"
          style:left="{left}px"
          style:top="{placed.top}px"
          style:width="{placed.width}px"
          style:height="{placed.height}px"
        >
          <ChangeMarks
            {side}
            page={slot.page}
            changes={changesOnPage.get(slot.page) ?? []}
            scale={zoom}
            pageSize={slot.size}
            {selectedId}
            {onSelect}
          />
        </div>
      {:else}
        <div
          class="missing"
          style:left="{left}px"
          style:top="{placed.top}px"
          style:width="{placed.width}px"
          style:height="{placed.height}px"
        >
          <span>{missingText}</span>
        </div>
      {/if}
    {/each}
  </div>
</div>

<style>
  .column {
    position: relative;
    flex: 1;
    min-width: 0;
    height: 100%;
    overflow: auto;
    background: var(--desk);
    outline: none;
  }

  .column:focus-visible {
    box-shadow: inset 0 0 0 2px var(--accent);
  }

  .content {
    position: relative;
  }

  .marks-layer {
    position: absolute;
  }

  /* where only the other version has a page: an empty dashed sheet */
  .missing {
    position: absolute;
    display: grid;
    place-items: center;
    border: 1.5px dashed var(--label-tertiary);
    border-radius: 2px;
    color: var(--label-secondary);
  }
</style>

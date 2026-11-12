<script lang="ts">
  // a small picture of every page, in a list that only builds the
  // thumbnails near its visible part: a 500 page document gets a dozen
  // images, not 500.
  //
  // the list is one keyboard stop. the arrows move through the document
  // itself, and the current page's thumbnail follows, so the focus and
  // the reading position never disagree.
  import { untrack } from "svelte";
  import { tileUrl } from "./api";
  import { remInPixels } from "./dom";
  import { PAGE_GAP, scrollToReveal, visiblePages, type PageSize } from "./layout";
  import { thumbnailKeyTarget, thumbnailLayout, thumbnailTiles } from "./thumbnails";
  import { retryTile } from "./tileRetry";

  interface Props {
    documentId: number;
    pages: PageSize[];
    /** the pages of the current row: one page, or a pair in two page mode */
    currentRow: { first: number; last: number };
    pixelRatio: number;
    /** changes after every edit, so the pictures are fetched again */
    revision: number;
    onGoToPage: (index: number) => void;
  }

  let { documentId, pages, currentRow, pixelRatio, revision, onGoToPage }: Props = $props();

  let list = $state<HTMLDivElement>();
  let scrollTop = $state(0);
  let listHeight = $state(0);

  // room under each picture for its page number: the subheadline line plus a little space
  const labelHeight = 1.25 * remInPixels();
  const layout = $derived(thumbnailLayout(pages, labelHeight));
  const shown = $derived.by(() => {
    // a screen's worth above and below is kept ready for smooth scrolling
    const range = visiblePages(layout.pages, scrollTop, listHeight, listHeight);
    return range ? layout.pages.slice(range.first, range.last + 1) : [];
  });

  const optionId = (index: number) => `thumbnail-${documentId}-${index}`;
  const isCurrent = (index: number) => index >= currentRow.first && index <= currentRow.last;

  function onKeyDown(event: KeyboardEvent) {
    const target = thumbnailKeyTarget(event.key, currentRow, pages.length);
    if (target === null) return;
    event.preventDefault();
    onGoToPage(target);
  }

  /** scrolls the list just enough to show these pages' thumbnails (one, or a pair). */
  function reveal(first: number, last: number) {
    const top = layout.pages[first];
    const bottom = layout.pages[last];
    if (!list || !top || !bottom) return;
    const span = bottom.top + bottom.height - top.top;
    const target = scrollToReveal(top.top, span, list.scrollTop, list.clientHeight, PAGE_GAP);
    if (target !== null) list.scrollTop = target;
  }

  // the current page's thumbnail stays in view as the reader moves through
  // the document. it only runs when the page changes, so scrolling the list
  // by hand to look around is never fought.
  $effect(() => {
    const { first, last } = currentRow;
    untrack(() => reveal(first, last));
  });
</script>

<div
  class="list"
  role="listbox"
  tabindex="0"
  aria-label="Pages"
  aria-activedescendant={optionId(currentRow.first)}
  bind:this={list}
  bind:clientHeight={listHeight}
  onscroll={() => (scrollTop = list?.scrollTop ?? 0)}
  onkeydown={onKeyDown}
>
  <div class="content" style:height="{layout.totalHeight}px">
    {#each shown as slot (slot.index)}
      {@const thumbnail = thumbnailTiles(pages[slot.index], pixelRatio)}
      <!-- options are not focusable on purpose: the listbox keeps the focus
           and handles the keys, and aria-activedescendant names the current
           page (the standard listbox pattern). a click still focuses the list. -->
      <!-- svelte-ignore a11y_interactive_supports_focus, a11y_click_events_have_key_events -->
      <div
        id={optionId(slot.index)}
        class="option"
        class:current={isCurrent(slot.index)}
        role="option"
        aria-selected={slot.index === currentRow.first}
        aria-posinset={slot.index + 1}
        aria-setsize={pages.length}
        aria-label={`Page ${slot.index + 1}`}
        style:top="{slot.top}px"
        onclick={() => onGoToPage(slot.index)}
      >
        <span class="sheet" style:width="{thumbnail.width}px" style:height="{thumbnail.height}px">
          {#each thumbnail.tiles as tile (`${thumbnail.scale}/${tile.column}/${tile.row}`)}
            <img
              src={tileUrl(documentId, slot.index, thumbnail.scale, tile.column, tile.row, revision)}
              alt=""
              draggable="false"
              onerror={retryTile}
              style:left="{tile.left}px"
              style:top="{tile.top}px"
              style:width="{tile.width}px"
              style:height="{tile.height}px"
            />
          {/each}
        </span>
        <span class="number" aria-hidden="true">{slot.index + 1}</span>
      </div>
    {/each}
  </div>
</div>

<style>
  .list {
    height: 100%;
    overflow-y: auto;
    border-radius: var(--radius-small);
  }

  /* the focus ring sits inside the list so the sidebar edge does not clip it */
  .list:focus-visible {
    outline-offset: -3px;
  }

  .content {
    position: relative;
  }

  .option {
    position: absolute;
    right: 0;
    left: 0;
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 0.375rem;
    outline: none;
  }

  .sheet {
    position: relative;
    overflow: hidden;
    border-radius: 2px;
    background: #ffffff;
    box-shadow: var(--shadow-sheet);
  }

  .sheet img {
    position: absolute;
    user-select: none;
  }

  .option:hover .sheet {
    box-shadow: var(--shadow-sheet-hover);
  }

  /* the page being read: the accent ring, the one strong mark in the sidebar */
  .option.current .sheet {
    box-shadow:
      0 0 0 2px var(--glass),
      0 0 0 4px var(--accent);
  }

  .number {
    padding: 0 0.375rem;
    border-radius: 0.4375rem;
    color: var(--label-secondary);
    font: var(--subheadline);
    font-variant-numeric: tabular-nums;
    letter-spacing: var(--subheadline-spacing);
  }

  .option.current .number {
    background: var(--accent);
    color: var(--on-accent);
    font-weight: 600;
  }
</style>

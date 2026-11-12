<script lang="ts">
  // a small picture of every page, in a list that only builds the
  // thumbnails near its visible part: a 500 page document gets a dozen
  // images, not 500.
  //
  // it is also where pages are chosen and rearranged:
  // - click selects a page (and goes to it), ctrl or command click adds or
  //   removes one, shift click selects a run (rules in selection.ts)
  // - the arrows move, shift with the arrows grows the selection, and
  //   ctrl or command + A selects every page
  // - dragging moves the selected pages; a line shows where they will land.
  //   the drag is worked out from the list's layout numbers, never from
  //   page elements, because the element under the pointer may not exist
  //   yet (see movePages.ts)
  // - right click opens a menu of page commands
  // - pdf files dragged in from the system are inserted where dropped
  //
  // the list is one keyboard stop, with aria-activedescendant naming the
  // focused page (the standard listbox pattern).
  import { onMount, untrack } from "svelte";
  import { tileUrl } from "./api";
  import { remInPixels } from "./dom";
  import { pdfsAmong, setFileDropTarget } from "./fileDrop";
  import { PAGE_GAP, scrollToReveal, visiblePages, type PageSize } from "./layout";
  import { autoScrollStep, changesNothing, DRAG_THRESHOLD_PX, dropIndexAt, insertionLineY } from "./movePages";
  import { clickPage, isSelected, moveByKey, selectAll, selectOnly, type PageSelection } from "./selection";
  import { thumbnailLayout, thumbnailTiles } from "./thumbnails";
  import { retryTile } from "./tileRetry";

  interface Props {
    documentId: number;
    pages: PageSize[];
    /** the pages of the current row: one page, or a pair in two page mode */
    currentRow: { first: number; last: number };
    pixelRatio: number;
    /** changes after every edit, so the pictures are fetched again */
    revision: number;
    selection: PageSelection;
    onSelectionChange: (selection: PageSelection) => void;
    onGoToPage: (index: number) => void;
    /** move `pages` as a block to just before the page now at `before` */
    onMovePages: (pages: number[], before: number) => void;
    /** right click or the menu key; the selection is already up to date */
    onContextMenu: (x: number, y: number) => void;
    /** pdf files dropped into the list, to insert before the page now at `atIndex` */
    onInsertFiles: (paths: string[], atIndex: number) => void;
  }

  let {
    documentId,
    pages,
    currentRow,
    pixelRatio,
    revision,
    selection,
    onSelectionChange,
    onGoToPage,
    onMovePages,
    onContextMenu,
    onInsertFiles,
  }: Props = $props();

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
  const focusPage = $derived(selection.focus ?? currentRow.first);
  // with nothing selected the page being read is marked, as before
  const isMarked = (index: number) =>
    selection.pages.length > 0 ? isSelected(selection, index) : index >= currentRow.first && index <= currentRow.last;

  // --- dragging pages ---

  /** a press that may become a drag once the pointer moves far enough */
  let press: { index: number; startY: number; keys: { toggle: boolean; extend: boolean } } | null = null;
  let drag = $state<{ pages: number[]; pointerY: number; dropIndex: number } | null>(null);
  /** where files dragged in from the system would go */
  let fileDropIndex = $state<number | null>(null);
  let scrollFrame = 0;

  const lineIndex = $derived(
    drag ? (changesNothing(pages.length, drag.pages, drag.dropIndex) ? null : drag.dropIndex) : fileDropIndex,
  );
  const lineY = $derived(lineIndex === null ? 0 : insertionLineY(layout.pages, lineIndex, PAGE_GAP));

  /** the pointer's height in the list's content, which scrolls */
  function contentY(clientY: number): number {
    const rect = list?.getBoundingClientRect();
    return rect ? clientY - rect.top + (list?.scrollTop ?? 0) : 0;
  }

  function insideList(point: { x: number; y: number }): boolean {
    const rect = list?.getBoundingClientRect();
    return !!rect && point.x >= rect.left && point.x <= rect.right && point.y >= rect.top && point.y <= rect.bottom;
  }

  function onPointerDown(event: PointerEvent, index: number) {
    if (event.button !== 0) return;
    press = { index, startY: event.clientY, keys: { toggle: event.ctrlKey || event.metaKey, extend: event.shiftKey } };
  }

  function onPointerMove(event: PointerEvent) {
    if (drag) {
      drag = { ...drag, pointerY: event.clientY, dropIndex: dropIndexAt(layout.pages, contentY(event.clientY)) };
      return;
    }
    if (!press || Math.abs(event.clientY - press.startY) < DRAG_THRESHOLD_PX) return;
    startDrag(press.index, event);
  }

  function startDrag(index: number, event: PointerEvent) {
    // dragging an unselected page moves just that page, and selects it
    let moving = selection.pages;
    if (!isSelected(selection, index)) {
      onSelectionChange(selectOnly(index));
      moving = [index];
    }
    // the list keeps the pointer even when the thumbnail under it is
    // dropped from the page by scrolling
    list?.setPointerCapture(event.pointerId);
    drag = { pages: moving, pointerY: event.clientY, dropIndex: dropIndexAt(layout.pages, contentY(event.clientY)) };
    press = null;
    scrollWhileDragging();
  }

  // near the top or bottom edge the list scrolls by itself, so a page can
  // be dragged anywhere in a long document
  function scrollWhileDragging() {
    cancelAnimationFrame(scrollFrame);
    scrollFrame = requestAnimationFrame(() => {
      if (!drag || !list) return;
      const rect = list.getBoundingClientRect();
      const step = autoScrollStep(drag.pointerY - rect.top, rect.height);
      if (step !== 0) {
        list.scrollTop += step;
        drag = { ...drag, dropIndex: dropIndexAt(layout.pages, contentY(drag.pointerY)) };
      }
      scrollWhileDragging();
    });
  }

  function onPointerUp() {
    if (drag) {
      const { pages: moving, dropIndex } = drag;
      endDrag();
      if (!changesNothing(pages.length, moving, dropIndex)) onMovePages(moving, dropIndex);
      return;
    }
    if (press) {
      const { index, keys } = press;
      press = null;
      onSelectionChange(clickPage(selection, index, keys));
      if (!keys.toggle && !keys.extend) onGoToPage(index);
    }
  }

  function endDrag() {
    drag = null;
    press = null;
    cancelAnimationFrame(scrollFrame);
  }

  // --- keys ---

  function onKeyDown(event: KeyboardEvent) {
    if (event.key === "Escape" && drag) {
      event.preventDefault();
      endDrag();
      return;
    }
    if ((event.ctrlKey || event.metaKey) && event.key.toLowerCase() === "a") {
      // select every page, not every word on the screen
      event.preventDefault();
      event.stopPropagation();
      onSelectionChange(selectAll(pages.length));
      return;
    }
    if (event.key === "ContextMenu" || (event.shiftKey && event.key === "F10")) {
      event.preventDefault();
      openMenuAtFocus();
      return;
    }
    // alt + arrows move pages (a window shortcut); ctrl or command + arrows are left alone
    if (event.altKey || event.ctrlKey || event.metaKey) return;
    const next = moveByKey(selection, event.key, pages.length, event.shiftKey, currentRow.first);
    if (!next) return;
    event.preventDefault();
    onSelectionChange(next);
    if (next.focus !== null) onGoToPage(next.focus);
  }

  function onContextMenuEvent(event: MouseEvent, index: number) {
    event.preventDefault();
    if (!isSelected(selection, index)) onSelectionChange(selectOnly(index));
    onContextMenu(event.clientX, event.clientY);
  }

  function openMenuAtFocus() {
    const page = selection.pages.length > 0 ? (selection.focus ?? selection.pages[0]) : focusPage;
    if (selection.pages.length === 0) onSelectionChange(selectOnly(page));
    const element = list?.querySelector(`#${optionId(page)}`);
    const rect = (element ?? list)?.getBoundingClientRect();
    if (rect) onContextMenu(rect.left + rect.width / 2, rect.top + rect.height / 2);
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
    untrack(() => {
      if (!drag) reveal(first, last);
    });
  });

  // pdfs dragged in from the system land where the line shows
  onMount(() =>
    setFileDropTarget({
      over: (x, y) => {
        const inside = insideList({ x, y });
        fileDropIndex = inside ? dropIndexAt(layout.pages, contentY(y)) : null;
        return inside;
      },
      drop: (paths, point) => {
        const pdfs = pdfsAmong(paths);
        // without a position, a drop counts as ours only if the line was showing
        let index = fileDropIndex;
        if (point) index = insideList(point) ? dropIndexAt(layout.pages, contentY(point.y)) : null;
        fileDropIndex = null;
        if (index === null || pdfs.length === 0) return false;
        onInsertFiles(pdfs, index);
        return true;
      },
      leave: () => (fileDropIndex = null),
    }),
  );
</script>

<div
  class="list"
  class:dragging={drag !== null}
  role="listbox"
  tabindex="0"
  aria-label="Pages"
  aria-multiselectable="true"
  aria-activedescendant={optionId(focusPage)}
  bind:this={list}
  bind:clientHeight={listHeight}
  onscroll={() => (scrollTop = list?.scrollTop ?? 0)}
  onkeydown={onKeyDown}
  onpointermove={onPointerMove}
  onpointerup={onPointerUp}
  onpointercancel={endDrag}
>
  <div class="content" style:height="{layout.totalHeight}px">
    {#each shown as slot (slot.index)}
      {@const thumbnail = thumbnailTiles(pages[slot.index], pixelRatio)}
      <!-- options are not focusable on purpose: the listbox keeps the focus
           and handles the keys, and aria-activedescendant names the focused
           page. a click still focuses the list. -->
      <!-- svelte-ignore a11y_interactive_supports_focus, a11y_click_events_have_key_events -->
      <div
        id={optionId(slot.index)}
        class="option"
        class:marked={isMarked(slot.index)}
        class:focused={slot.index === focusPage && selection.pages.length > 0}
        class:moving={drag !== null && drag.pages.includes(slot.index)}
        role="option"
        aria-selected={isSelected(selection, slot.index)}
        aria-posinset={slot.index + 1}
        aria-setsize={pages.length}
        aria-label={`Page ${slot.index + 1}`}
        style:top="{slot.top}px"
        onpointerdown={(event) => onPointerDown(event, slot.index)}
        oncontextmenu={(event) => onContextMenuEvent(event, slot.index)}
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
    {#if lineIndex !== null}
      <div class="insertion" style:top="{lineY}px" aria-hidden="true"></div>
    {/if}
  </div>
  {#if drag}
    <!-- so a screen reader knows a move is under way, and where it lands -->
    <span class="visually-hidden" aria-live="polite">
      Moving {drag.pages.length} {drag.pages.length === 1 ? "page" : "pages"}. Drop before page {drag.dropIndex + 1}.
    </span>
  {/if}
</div>

<style>
  .list {
    height: 100%;
    overflow-y: auto;
    border-radius: var(--radius-small);
    touch-action: none;
  }

  .list.dragging {
    cursor: grabbing;
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
    user-select: none;
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

  /* selected pages (or the page being read): the accent ring, the one
     strong mark in the sidebar */
  .option.marked .sheet {
    box-shadow:
      0 0 0 2px var(--glass),
      0 0 0 4px var(--accent);
  }

  /* the keyboard's place inside a selection */
  .list:focus-visible .option.focused .sheet {
    box-shadow:
      0 0 0 2px var(--glass),
      0 0 0 4px var(--accent),
      0 0 0 7px var(--accent-soft);
  }

  .option.moving {
    opacity: 0.45;
  }

  .number {
    padding: 0 0.375rem;
    border-radius: 0.4375rem;
    color: var(--label-secondary);
    font: var(--subheadline);
    font-variant-numeric: tabular-nums;
    letter-spacing: var(--subheadline-spacing);
  }

  .option.marked .number {
    background: var(--accent);
    color: var(--on-accent);
    font-weight: 600;
  }

  /* where dragged pages (or dropped files) will land */
  .insertion {
    position: absolute;
    right: 0.5rem;
    left: 0.5rem;
    height: 3px;
    margin-top: -1.5px;
    border-radius: 1.5px;
    background: var(--accent);
    pointer-events: none;
  }
</style>

<script lang="ts">
  // shows one document as a scrolling column of pages.
  //
  // only the pages near the screen exist in the page at all, and each of
  // those only loads the tiles that are visible. that is what keeps a
  // 500 page file as light as a 5 page one.
  //
  // the toolbar floats above the pages instead of sitting in its own
  // strip, so the document reaches the top of the window (docs/DESIGN.md).
  import { tick, untrack } from "svelte";
  import { tileUrl, type OpenedDocument } from "./api";
  import HelpPanel from "./HelpPanel.svelte";
  import {
    anchorAt,
    PAGE_GAP,
    pageAtMiddle,
    placePages,
    positionOf,
    scaleMilli,
    tilesForPage,
    visiblePages,
    zoomIn,
    zoomOut,
    zoomToFitWidth,
  } from "./layout";
  import ToolButton from "./ToolButton.svelte";
  import { tooltipFor, tools } from "./tools";

  interface Props {
    document: OpenedDocument;
    onRecents: () => void;
    onNewWindow: () => void;
  }

  let { document: doc, onRecents, onNewWindow }: Props = $props();

  let scroller = $state<HTMLDivElement>();
  let zoom = $state(1);
  let scrollTop = $state(0);
  let viewportWidth = $state(0);
  let viewportHeight = $state(0);
  let toolbarHeight = $state(0);
  let nightMode = $state(false);
  let showExplain = $state(false);
  // true until the reader picks their own zoom. while it is true the
  // page keeps filling the width, even when the window is resized.
  let followsWidth = true;
  // on a high density screen one css pixel is several real pixels;
  // tiles are rendered at the real size so text stays sharp
  let pixelRatio = $state(window.devicePixelRatio || 1);

  const layout = $derived(placePages(doc.pages, zoom, toolbarHeight));
  const scale = $derived(scaleMilli(zoom, pixelRatio));
  const contentWidth = $derived(Math.max(layout.widestPage + 2 * PAGE_GAP, viewportWidth));
  const currentPage = $derived(pageAtMiddle(layout.pages, scrollTop, viewportHeight) + 1);

  // pages within half a screen of the visible area are kept ready
  const shownPages = $derived.by(() => {
    const range = visiblePages(layout.pages, scrollTop, viewportHeight, viewportHeight / 2);
    return range ? layout.pages.slice(range.first, range.last + 1) : [];
  });

  function tilesOf(pageIndex: number, pageTop: number) {
    const margin = viewportHeight / 2;
    return tilesForPage(
      doc.pages[pageIndex],
      scale,
      pixelRatio,
      scrollTop - pageTop - margin,
      scrollTop + viewportHeight - pageTop + margin,
    );
  }

  /** a zoom the reader chose by hand: stop following the window's width. */
  function setZoom(newZoom: number) {
    followsWidth = false;
    applyZoom(newZoom);
  }

  /**
   * changes the zoom while keeping the same part of the document on screen.
   *
   * the spot just below the toolbar is remembered as "this far down that
   * page" and scrolled back to once the pages have their new sizes.
   */
  async function applyZoom(newZoom: number) {
    if (!scroller || newZoom === zoom) return;
    const anchor = anchorAt(layout.pages, scrollTop + toolbarHeight);
    zoom = newZoom;
    await tick();
    scroller.scrollTop = positionOf(layout.pages, anchor) - toolbarHeight;
  }

  function fitWidth() {
    followsWidth = true;
    applyZoom(zoomToFitWidth(doc.pages, viewportWidth));
  }

  function goToPage(pageNumber: number) {
    if (!scroller || !Number.isFinite(pageNumber)) return;
    const index = Math.min(Math.max(Math.round(pageNumber), 1), doc.pages.length) - 1;
    // stop with the page's top edge just below the floating toolbar
    scroller.scrollTop = layout.pages[index].top - PAGE_GAP - toolbarHeight;
  }

  function onPageInput(event: KeyboardEvent) {
    if (event.key !== "Enter") return;
    goToPage(Number((event.currentTarget as HTMLInputElement).value));
  }

  function onKeyDown(event: KeyboardEvent) {
    if (!(event.ctrlKey || event.metaKey)) return;
    if (event.key === "=" || event.key === "+") setZoom(zoomIn(zoom));
    else if (event.key === "-") setZoom(zoomOut(zoom));
    else if (event.key === "0") fitWidth();
    else return;
    event.preventDefault();
  }

  /**
   * a tile can fail when it was skipped during fast scrolling. if it is
   * still on screen it is simply asked for again, a few times at most.
   */
  function retryTile(event: Event) {
    const image = event.currentTarget as HTMLImageElement;
    // a tile that has left the screen (or whose document was closed) is not needed
    if (!image.isConnected) return;
    const attempts = Number(image.dataset.attempts ?? 0);
    if (attempts >= 4) return;
    image.dataset.attempts = String(attempts + 1);
    const address = image.src.split("#")[0];
    // the part after # is not sent anywhere: it only makes the address
    // look new so the image is loaded again
    setTimeout(
      () => {
        if (image.isConnected) image.src = `${address}#${attempts + 1}`;
      },
      250 * (attempts + 1),
    );
  }

  // start with the page filling the window's width, the most readable
  // size, and keep it that way as the window (or the side panel) changes
  $effect(() => {
    const width = viewportWidth;
    if (width > 0 && followsWidth) {
      // untrack: this should run when the width changes, not on every scroll
      untrack(() => applyZoom(zoomToFitWidth(doc.pages, width)));
    }
  });

  // ctrl + mouse wheel zooms. the listener is added by hand because it
  // must be allowed to cancel the browser's own page zoom.
  $effect(() => {
    const element = scroller;
    if (!element) return;
    const onWheel = (event: WheelEvent) => {
      if (!event.ctrlKey) return;
      event.preventDefault();
      setZoom(event.deltaY < 0 ? zoomIn(zoom) : zoomOut(zoom));
    };
    element.addEventListener("wheel", onWheel, { passive: false });
    return () => element.removeEventListener("wheel", onWheel);
  });

  // moving the window to another monitor can change the pixel density
  $effect(() => {
    const update = () => (pixelRatio = window.devicePixelRatio || 1);
    window.addEventListener("resize", update);
    return () => window.removeEventListener("resize", update);
  });
</script>

<svelte:window onkeydown={onKeyDown} />

<div class="viewer">
  <div class="stage">
    <!-- svelte-ignore a11y_no_noninteractive_tabindex -->
    <div
      class="scroller"
      class:night={nightMode}
      tabindex="0"
      role="document"
      aria-label={`${doc.name}, page ${currentPage} of ${doc.pages.length}`}
      bind:this={scroller}
      bind:clientWidth={viewportWidth}
      bind:clientHeight={viewportHeight}
      onscroll={() => (scrollTop = scroller?.scrollTop ?? 0)}
    >
      <div class="content" style:width="{contentWidth}px" style:height="{layout.totalHeight}px">
        {#each shownPages as page (page.index)}
          <div
            class="page"
            role="img"
            aria-label={`Page ${page.index + 1}`}
            style:top="{page.top}px"
            style:left="{(contentWidth - page.width) / 2}px"
            style:width="{page.width}px"
            style:height="{page.height}px"
          >
            {#each tilesOf(page.index, page.top) as tile (`${scale}/${tile.column}/${tile.row}`)}
              <img
                src={tileUrl(doc.id, page.index, scale, tile.column, tile.row)}
                alt=""
                draggable="false"
                onerror={retryTile}
                style:left="{tile.left}px"
                style:top="{tile.top}px"
                style:width="{tile.width}px"
                style:height="{tile.height}px"
              />
            {/each}
          </div>
        {/each}
      </div>
    </div>

    <!-- three areas with fixed jobs: where you are, how you see it, what else you can do -->
    <div class="toolbar" role="toolbar" aria-label="Document tools" bind:clientHeight={toolbarHeight}>
      <div class="area leading">
        <div class="glass group">
          <ToolButton name="recents" icon="back" symbolOnly onClick={onRecents} />
          <div class="title">
            <span class="name" title={doc.path}>{doc.name}</span>
            <label class="page-jump" title={tooltipFor("goToPage")}>
              <span>{tools.goToPage.label}</span>
              <input
                type="number"
                min="1"
                max={doc.pages.length}
                value={currentPage}
                onkeydown={onPageInput}
              />
              <span>of {doc.pages.length}</span>
            </label>
          </div>
        </div>
      </div>

      <div class="area">
        <div class="glass group">
          <ToolButton name="zoomOut" icon="zoomOut" onClick={() => setZoom(zoomOut(zoom))} />
          <span class="zoom-level" aria-live="polite" title="How big the page is shown">
            {Math.round(zoom * 100)}%
          </span>
          <ToolButton name="zoomIn" icon="zoomIn" onClick={() => setZoom(zoomIn(zoom))} />
          <ToolButton name="fitWidth" icon="fitWidth" onClick={fitWidth} />
        </div>
      </div>

      <div class="area trailing">
        <div class="glass group">
          <ToolButton
            name="nightMode"
            icon="nightMode"
            pressed={nightMode}
            onClick={() => (nightMode = !nightMode)}
          />
          <ToolButton name="newWindow" icon="newWindow" onClick={onNewWindow} />
          <ToolButton
            name="explain"
            icon="sidePanel"
            pressed={showExplain}
            onClick={() => (showExplain = !showExplain)}
          />
        </div>
      </div>
    </div>
  </div>

  {#if showExplain}
    <HelpPanel
      names={["recents", "goToPage", "zoomOut", "zoomIn", "fitWidth", "nightMode", "newWindow", "explain"]}
      onClose={() => (showExplain = false)}
    />
  {/if}
</div>

<style>
  .viewer {
    display: flex;
    height: 100%;
  }

  /* the document and the toolbar floating over it */
  .stage {
    position: relative;
    flex: 1;
    min-width: 0;
  }

  .scroller {
    width: 100%;
    height: 100%;
    overflow: auto;
    outline: none;
    box-shadow: none;
    background: var(--desk);
  }

  .content {
    position: relative;
  }

  .page {
    position: absolute;
    overflow: hidden;
    border-radius: 2px;
    background: #ffffff;
    box-shadow: var(--shadow-page);
  }

  .page img {
    position: absolute;
    user-select: none;
  }

  /* night mode flips light and dark, then turns the colours back around
     so blue stays blue. it is only a screen effect. */
  .night .page {
    filter: invert(0.92) hue-rotate(180deg);
  }

  .toolbar {
    position: absolute;
    inset: 0 0.75rem auto 0;
    display: grid;
    grid-template-columns: 1fr auto 1fr;
    align-items: start;
    gap: 0.75rem;
    padding: 0.625rem 0 0.625rem 0.75rem;
    /* clicks between the groups go through to the document */
    pointer-events: none;
  }

  /* pages fade out as they slide under the toolbar, so the controls
     stay readable without a hard bar across the window */
  .toolbar::before {
    content: "";
    position: absolute;
    inset: 0 0 -0.75rem 0;
    background: linear-gradient(to bottom, var(--desk) 35%, transparent);
    opacity: 0.9;
  }

  .area {
    position: relative;
    display: flex;
    min-width: 0;
  }

  .area.trailing {
    justify-content: flex-end;
  }

  .group {
    display: flex;
    align-items: center;
    gap: 0.125rem;
    min-width: 0;
    padding: var(--group-padding);
    pointer-events: auto;
  }

  .title {
    display: flex;
    flex-direction: column;
    gap: 0.125rem;
    min-width: 0;
    padding: 0 0.75rem 0 0.25rem;
  }

  .name {
    overflow: hidden;
    font: var(--headline);
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .page-jump {
    display: flex;
    align-items: center;
    gap: 0.25rem;
    color: var(--label-secondary);
    font: var(--subheadline);
    letter-spacing: var(--subheadline-spacing);
    white-space: nowrap;
  }

  /* looks like plain text until you point at it or click into it */
  .page-jump input {
    width: 2.5rem;
    padding: 0 0.125rem;
    border: 0;
    border-radius: 0.25rem;
    background: var(--control);
    color: var(--label);
    font: var(--subheadline);
    font-variant-numeric: tabular-nums;
    text-align: center;
    appearance: textfield;
  }

  .page-jump input::-webkit-inner-spin-button {
    appearance: none;
  }

  .page-jump input:hover,
  .page-jump input:focus {
    background: var(--control-hover);
  }

  .zoom-level {
    min-width: 2.75rem;
    color: var(--label-secondary);
    font: var(--subheadline);
    font-variant-numeric: tabular-nums;
    text-align: center;
  }

  /* on a narrow window the zoom number gives way so the buttons still fit */
  @media (max-width: 50rem) {
    .zoom-level {
      display: none;
    }

    .toolbar {
      grid-template-columns: minmax(0, 1fr) auto auto;
    }
  }
</style>

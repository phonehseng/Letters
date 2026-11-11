<script lang="ts">
  // shows one document as a scrolling column of pages.
  //
  // only the pages near the screen exist in the page at all, and each of
  // those only loads the tiles that are visible. that is what keeps a
  // 500 page file as light as a 5 page one.
  import { tick, untrack } from "svelte";
  import { tileUrl, type OpenedDocument } from "./api";
  import HelpPanel from "./HelpPanel.svelte";
  import {
    PAGE_GAP,
    pageAtMiddle,
    placePages,
    scaleMilli,
    tilesForPage,
    visiblePages,
    zoomIn,
    zoomOut,
    zoomToFitWidth,
  } from "./layout";
  import ToolButton from "./ToolButton.svelte";
  import { tools } from "./tools";

  interface Props {
    document: OpenedDocument;
    onHome: () => void;
    onNewWindow: () => void;
  }

  let { document: doc, onHome, onNewWindow }: Props = $props();

  let scroller = $state<HTMLDivElement>();
  let zoom = $state(1);
  let scrollTop = $state(0);
  let viewportWidth = $state(0);
  let viewportHeight = $state(0);
  let nightMode = $state(false);
  let showHelp = $state(false);
  // true until the reader picks their own zoom. while it is true the
  // page keeps filling the width, even when the window is resized.
  let followsWidth = true;
  // on a high density screen one css pixel is several real pixels;
  // tiles are rendered at the real size so text stays sharp
  let pixelRatio = $state(window.devicePixelRatio || 1);

  const layout = $derived(placePages(doc.pages, zoom));
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

  /** changes the zoom while keeping the same part of the document on screen. */
  async function applyZoom(newZoom: number) {
    if (!scroller || newZoom === zoom) return;
    const ratio = newZoom / zoom;
    zoom = newZoom;
    await tick();
    scroller.scrollTop = scrollTop * ratio;
  }

  function fitWidth() {
    followsWidth = true;
    applyZoom(zoomToFitWidth(doc.pages, viewportWidth));
  }

  function goToPage(pageNumber: number) {
    if (!scroller || !Number.isFinite(pageNumber)) return;
    const index = Math.min(Math.max(Math.round(pageNumber), 1), doc.pages.length) - 1;
    scroller.scrollTop = layout.pages[index].top - PAGE_GAP;
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
    const attempts = Number(image.dataset.attempts ?? 0);
    if (attempts >= 4) return;
    image.dataset.attempts = String(attempts + 1);
    const address = image.src.split("#")[0];
    // the part after # is not sent anywhere: it only makes the address
    // look new so the image is loaded again
    setTimeout(() => (image.src = `${address}#${attempts + 1}`), 250 * (attempts + 1));
  }

  // start with the page filling the window's width, the most readable
  // size, and keep it that way as the window (or the help panel) changes
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
  <div class="bar" role="toolbar" aria-label="Document tools">
    <div class="side">
      <ToolButton name="home" icon="back" onClick={onHome} />
    </div>

    <div class="title">
      <span class="file-name" title={doc.path}>{doc.name}</span>
      <label class="page-jump" title={tools.goToPage.tooltip}>
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

    <div class="side end">
      <ToolButton name="zoomOut" icon="zoomOut" onClick={() => setZoom(zoomOut(zoom))} />
      <span class="zoom-level" aria-live="polite" title="How big the page is shown">
        {Math.round(zoom * 100)}%
      </span>
      <ToolButton name="zoomIn" icon="zoomIn" onClick={() => setZoom(zoomIn(zoom))} />
      <ToolButton name="fitWidth" icon="fitWidth" onClick={fitWidth} />
      <span class="divider"></span>
      <ToolButton
        name="nightMode"
        icon="nightMode"
        pressed={nightMode}
        onClick={() => (nightMode = !nightMode)}
      />
      <ToolButton name="newWindow" icon="newWindow" onClick={onNewWindow} />
      <ToolButton name="help" icon="help" pressed={showHelp} onClick={() => (showHelp = !showHelp)} />
    </div>
  </div>

  <div class="body">
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

    {#if showHelp}
      <HelpPanel
        names={["home", "goToPage", "zoomOut", "zoomIn", "fitWidth", "nightMode", "newWindow", "help"]}
        onClose={() => (showHelp = false)}
      />
    {/if}
  </div>
</div>

<style>
  .viewer {
    display: flex;
    flex-direction: column;
    height: 100%;
  }

  /* three parts: back on the left, the title in the middle, tools on
     the right. the middle stays centred whatever the sides contain. */
  .bar {
    display: grid;
    grid-template-columns: 1fr auto 1fr;
    align-items: center;
    gap: 0.5rem;
    padding: 0.35rem 0.75rem;
    border-bottom: 0.5px solid var(--hairline);
    background: var(--bar);
    backdrop-filter: blur(24px) saturate(1.8);
  }

  .side {
    display: flex;
    align-items: center;
    gap: 0.1rem;
    min-width: 0;
  }

  .side.end {
    justify-content: flex-end;
  }

  .title {
    display: flex;
    flex-direction: column;
    align-items: center;
    min-width: 0;
    max-width: 32vw;
  }

  .file-name {
    max-width: 100%;
    overflow: hidden;
    font-size: 0.8125rem;
    font-weight: 600;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .page-jump {
    display: flex;
    align-items: center;
    gap: 0.3rem;
    color: var(--text-secondary);
    font-size: 0.6875rem;
    white-space: nowrap;
  }

  /* looks like plain text until you point at it or click into it */
  .page-jump input {
    width: 2.6rem;
    padding: 0.05rem 0.2rem;
    border: 0;
    border-radius: 0.25rem;
    background: transparent;
    color: var(--text);
    font-size: 0.6875rem;
    font-variant-numeric: tabular-nums;
    text-align: center;
    appearance: textfield;
  }

  .page-jump input::-webkit-inner-spin-button {
    appearance: none;
  }

  .page-jump input:hover,
  .page-jump input:focus {
    background: var(--control);
  }

  .zoom-level {
    min-width: 2.6rem;
    color: var(--text-secondary);
    font-size: 0.75rem;
    font-variant-numeric: tabular-nums;
    text-align: center;
  }

  .divider {
    width: 0.5px;
    height: 1.6rem;
    margin: 0 0.4rem;
    background: var(--hairline);
  }

  .body {
    display: flex;
    flex: 1;
    min-height: 0;
  }

  .scroller {
    flex: 1;
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

  /* on a narrow window the title gives way so the tools still fit */
  @media (max-width: 52rem) {
    .bar {
      grid-template-columns: auto 1fr auto;
    }

    .title {
      max-width: none;
    }

    .zoom-level,
    .divider {
      display: none;
    }
  }
</style>

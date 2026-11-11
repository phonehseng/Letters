<script lang="ts">
  // shows one document as a scrolling column of pages, or of pairs.
  //
  // only the pages near the screen exist in the page at all, and each of
  // those only loads the tiles that are visible. that is what keeps a
  // 500 page file as light as a 5 page one.
  //
  // the toolbar floats above the pages instead of sitting in its own
  // strip, so the document reaches the top of the window (docs/DESIGN.md).
  // this file holds the viewer's state; the parts are their own components.
  import { tick, untrack } from "svelte";
  import { documentOutline, type OpenedDocument, type OutlineItem, type SearchMatch } from "./api";
  import { appCommands, type Command } from "./commands";
  import { remInPixels } from "./dom";
  import FindBar from "./FindBar.svelte";
  import HelpPanel from "./HelpPanel.svelte";
  import {
    anchorAt,
    PAGE_GAP,
    pageAtMiddle,
    placePages,
    positionOf,
    rowAround,
    scaleMilli,
    scrollToCentre,
    scrollTopForPage,
    tilesForPage,
    visiblePages,
    zoomIn,
    zoomOut,
    zoomToFitWidth,
    type PageMode,
  } from "./layout";
  import PageView from "./PageView.svelte";
  import { highlightsOnPage, matchBounds, matchesByPage } from "./search";
  import SearchHighlights from "./SearchHighlights.svelte";
  import Sidebar from "./Sidebar.svelte";
  import {
    CLOSED_SIDEBAR,
    onlyOnePanelFits,
    toggleSidebar,
    toggleSidebarMode,
    type SidebarMode,
    type SidebarState,
  } from "./sidebar";
  import { hiddenParts } from "./toolbar";
  import type { ToolName } from "./tools";
  import ViewerToolbar from "./ViewerToolbar.svelte";

  interface Props {
    document: OpenedDocument;
    onRecents: () => void;
    onNewWindow: () => void;
  }

  let { document: doc, onRecents, onNewWindow }: Props = $props();

  let scroller = $state<HTMLDivElement>();
  let zoom = $state(1);
  let pageMode = $state<PageMode>("single");
  let scrollTop = $state(0);
  let viewportWidth = $state(0);
  let viewportHeight = $state(0);
  let windowWidth = $state(window.innerWidth);
  let toolbarHeight = $state(0);
  let nightMode = $state(false);
  let showExplain = $state(false);
  let sidebar = $state<SidebarState>(CLOSED_SIDEBAR);
  let outline = $state<OutlineItem[] | null>(null);
  let findOpen = $state(false);
  let findBar = $state<FindBar>();
  let found = $state<{ matches: SearchMatch[]; current: number }>({ matches: [], current: -1 });
  // true until the reader picks their own zoom. while it is true the
  // page keeps filling the width, even when the window is resized.
  let followsWidth = true;
  let outlineRequested = false;
  // on a high density screen one css pixel is several real pixels;
  // tiles are rendered at the real size so text stays sharp
  let pixelRatio = $state(window.devicePixelRatio || 1);
  let remPx = $state(remInPixels());

  const layout = $derived(placePages(doc.pages, zoom, toolbarHeight, pageMode));
  const scale = $derived(scaleMilli(zoom, pixelRatio));
  const contentWidth = $derived(Math.max(layout.widestRow + 2 * PAGE_GAP, viewportWidth));
  const currentIndex = $derived(pageAtMiddle(layout.pages, scrollTop, viewportHeight));
  const currentRow = $derived(rowAround(layout.pages, currentIndex));
  const hidden = $derived(hiddenParts(viewportWidth, remPx));
  const matchesOnPages = $derived(matchesByPage(found.matches));

  // pages within half a screen of the visible area are kept ready
  const shownPages = $derived.by(() => {
    const range = visiblePages(layout.pages, scrollTop, viewportHeight, viewportHeight / 2);
    return range ? layout.pages.slice(range.first, range.last + 1) : [];
  });

  // the explain panel describes what is on screen right now
  const explained = $derived<ToolName[]>([
    "sidebar",
    "thumbnails",
    "outline",
    "recents",
    "goToPage",
    "zoomOut",
    "zoomIn",
    "fitWidth",
    "find",
    ...(findOpen ? (["findNext", "findPrevious"] as const) : []),
    "view",
    "twoPages",
    "nightMode",
    "newWindow",
    "explain",
  ]);

  /** where a page's left edge is in the scroll area */
  const pageLeft = (pageLeftFromCentre: number) => contentWidth / 2 + pageLeftFromCentre;

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

  /**
   * changes the zoom or the page mode while keeping the same part of the
   * document on screen.
   *
   * the spot just below the toolbar is remembered as "this far down that
   * page" and scrolled back to once the pages have their new places.
   */
  async function relayout(newZoom: number, newMode: PageMode) {
    if (!scroller || (newZoom === zoom && newMode === pageMode)) return;
    const anchor = anchorAt(layout.pages, scrollTop + toolbarHeight);
    zoom = newZoom;
    pageMode = newMode;
    await tick();
    scroller.scrollTop = positionOf(layout.pages, anchor) - toolbarHeight;
  }

  /** a zoom the reader chose by hand: stop following the window's width. */
  function setZoom(newZoom: number) {
    followsWidth = false;
    relayout(newZoom, pageMode);
  }

  function fitWidth() {
    followsWidth = true;
    relayout(zoomToFitWidth(doc.pages, viewportWidth, pageMode), pageMode);
  }

  function toggleTwoPages() {
    const mode: PageMode = pageMode === "single" ? "twoPages" : "single";
    // a page that filled the width becomes a pair that fills it
    relayout(followsWidth ? zoomToFitWidth(doc.pages, viewportWidth, mode) : zoom, mode);
  }

  /** goes to a page, counted from 0. */
  function goToPage(index: number) {
    if (!scroller || !Number.isFinite(index)) return;
    scroller.scrollTop = scrollTopForPage(layout.pages, index, toolbarHeight);
  }

  // on a small window the sidebar and the explain panel take turns
  function setSidebar(next: SidebarState) {
    if (next.open && onlyOnePanelFits(windowWidth, remPx)) showExplain = false;
    sidebar = next;
  }

  function toggleExplain() {
    showExplain = !showExplain;
    if (showExplain && onlyOnePanelFits(windowWidth, remPx)) sidebar = { ...sidebar, open: false };
  }

  function openFind() {
    if (findOpen) findBar?.focus();
    else findOpen = true;
  }

  function closeFind() {
    findOpen = false;
    found = { matches: [], current: -1 };
    scroller?.focus();
  }

  function onFindChange(matches: SearchMatch[], current: number) {
    found = { matches, current };
    revealMatch(current);
  }

  /**
   * scrolls the current match to the middle of the screen if it is not
   * already in view. the band under the toolbar and the find bar does
   * not count as in view, since the match would be hidden behind them.
   */
  function revealMatch(index: number) {
    const match = found.matches[index];
    const page = match && layout.pages[match.page];
    const bounds = match && matchBounds(match);
    if (!scroller || !page || !bounds) return;
    const covered = toolbarHeight + 3 * remPx;
    const top = page.top + bounds.y * zoom;
    const y = scrollToCentre(top - covered, bounds.height * zoom, scroller.scrollTop, viewportHeight - covered);
    if (y !== null) scroller.scrollTop = y;
    const left = pageLeft(page.left) + bounds.x * zoom;
    const x = scrollToCentre(left, bounds.width * zoom, scroller.scrollLeft, viewportWidth);
    if (x !== null) scroller.scrollLeft = x;
  }

  function onWindowKeyDown(event: KeyboardEvent) {
    // escape closes find from anywhere in the viewer, not only its field
    if (event.key === "Escape" && findOpen && !event.defaultPrevented) closeFind();
  }

  // the menu bar and the keyboard shortcuts reach the viewer through
  // the shared dispatcher (see commands.ts)
  $effect(() => {
    const handlers: [Command, () => void][] = [
      ["zoom-in", () => setZoom(zoomIn(zoom))],
      ["zoom-out", () => setZoom(zoomOut(zoom))],
      ["fit-width", fitWidth],
      ["find", openFind],
      ["toggle-thumbnails", () => setSidebar(toggleSidebarMode(sidebar, "thumbnails"))],
      ["toggle-outline", () => setSidebar(toggleSidebarMode(sidebar, "outline"))],
      ["toggle-two-pages", toggleTwoPages],
      ["toggle-night-mode", () => (nightMode = !nightMode)],
    ];
    const takeBack = handlers.map(([command, handler]) => appCommands.register(command, handler));
    return () => takeBack.forEach((unregister) => unregister());
  });

  // the outline is read the first time it is shown, then kept
  $effect(() => {
    if (!sidebar.open || sidebar.mode !== "outline" || outlineRequested) return;
    outlineRequested = true;
    untrack(() =>
      documentOutline(doc.id)
        .then((items) => (outline = items))
        // a document whose outline cannot be read is treated as having none
        .catch(() => (outline = [])),
    );
  });

  // start with the page filling the window's width, the most readable
  // size, and keep it that way as the window (or a side panel) changes
  $effect(() => {
    const width = viewportWidth;
    if (width > 0 && followsWidth) {
      // untrack: this should run when the width changes, not on every scroll
      untrack(() => relayout(zoomToFitWidth(doc.pages, width, pageMode), pageMode));
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

  // moving the window to another monitor can change the pixel density,
  // and the system text size can change the size of a rem
  $effect(() => {
    const update = () => {
      pixelRatio = window.devicePixelRatio || 1;
      remPx = remInPixels();
    };
    window.addEventListener("resize", update);
    return () => window.removeEventListener("resize", update);
  });
</script>

<svelte:window bind:innerWidth={windowWidth} onkeydown={onWindowKeyDown} />

<div class="viewer">
  {#if sidebar.open}
    <Sidebar
      mode={sidebar.mode}
      documentId={doc.id}
      pages={doc.pages}
      {currentRow}
      {pixelRatio}
      {outline}
      onModeChange={(mode: SidebarMode) => setSidebar({ open: true, mode })}
      onGoToPage={goToPage}
    />
  {/if}

  <div class="stage">
    <!-- svelte-ignore a11y_no_noninteractive_tabindex -->
    <div
      class="scroller"
      class:night={nightMode}
      tabindex="0"
      role="document"
      aria-label={`${doc.name}, page ${currentIndex + 1} of ${doc.pages.length}`}
      bind:this={scroller}
      bind:clientWidth={viewportWidth}
      bind:clientHeight={viewportHeight}
      onscroll={() => (scrollTop = scroller?.scrollTop ?? 0)}
    >
      <div class="content" style:width="{contentWidth}px" style:height="{layout.totalHeight}px">
        {#each shownPages as page (page.index)}
          <PageView
            documentId={doc.id}
            {page}
            left={pageLeft(page.left)}
            {scale}
            tiles={tilesOf(page.index, page.top)}
          />
          {#if matchesOnPages.has(page.index)}
            <SearchHighlights
              highlights={highlightsOnPage(found.matches, matchesOnPages, page.index, zoom, found.current)}
              left={pageLeft(page.left)}
              top={page.top}
              width={page.width}
              height={page.height}
            />
          {/if}
        {/each}
      </div>
    </div>

    <ViewerToolbar
      name={doc.name}
      path={doc.path}
      pageCount={doc.pages.length}
      currentPage={currentIndex + 1}
      {zoom}
      {hidden}
      sidebarOpen={sidebar.open}
      {findOpen}
      explainOpen={showExplain}
      twoPages={pageMode === "twoPages"}
      {nightMode}
      bind:height={toolbarHeight}
      onSidebar={() => setSidebar(toggleSidebar(sidebar))}
      {onRecents}
      onGoToPage={(pageNumber) => goToPage(pageNumber - 1)}
      onZoomOut={() => setZoom(zoomOut(zoom))}
      onZoomIn={() => setZoom(zoomIn(zoom))}
      onFitWidth={fitWidth}
      onFind={() => (findOpen ? closeFind() : openFind())}
      onToggleTwoPages={toggleTwoPages}
      onToggleNightMode={() => (nightMode = !nightMode)}
      {onNewWindow}
      onExplain={toggleExplain}
    />

    {#if findOpen}
      <FindBar
        bind:this={findBar}
        documentId={doc.id}
        currentPage={currentIndex}
        top={toolbarHeight}
        onChange={onFindChange}
        onClose={closeFind}
      />
    {/if}
  </div>

  {#if showExplain}
    <HelpPanel names={explained} onClose={() => (showExplain = false)} />
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
</style>

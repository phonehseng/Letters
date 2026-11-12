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
  //
  // the document can change (page tools, undo, redo). every change comes
  // back from the shell as a new DocumentState, which the window passes
  // down again as `document`; page sizes, tiles, thumbnails, the outline
  // and find all redraw from it, and the reader's place is kept.
  //
  // page tools act on the pages selected in the thumbnails, or on the
  // page being read when none are. the Tools inspector, the Explain panel
  // and the sidebar share the window by the rules in sidePanels.ts.
  import { tick, untrack } from "svelte";
  import {
    asCommandError,
    chooseFile,
    chooseSaveLocation,
    documentOutline,
    newWindow,
    redo,
    undo,
    type DocumentState,
    type OutlineItem,
    type SearchMatch,
  } from "./api";
  import { appCommands, type Command } from "./commands";
  import ContextMenu, { type ContextItem } from "./ContextMenu.svelte";
  import CropOverlay from "./CropOverlay.svelte";
  import { startingCrop, type CropBox } from "./cropRect";
  import { isAvailable } from "./documentMenu";
  import { documentChanges, placeAfterEdit } from "./documentState";
  import { remInPixels } from "./dom";
  import FindBar from "./FindBar.svelte";
  import HelpPanel from "./HelpPanel.svelte";
  import { explainGroups, tabById, tabWithTool, type ToolContext } from "./inspector";
  import InspectorPanel from "./InspectorPanel.svelte";
  import { positionsAfterMove, stepTarget } from "./movePages";
  import {
    deletePages,
    extractPages,
    fileNameOf,
    insertBlankPages,
    insertPagesFrom,
    movePages,
    pathBeside,
    pdfPageCount,
    rotatePages,
    stemOf,
  } from "./pageApi";
  import { describePages, pagesAsTypedText } from "./pageRanges";
  import { keepWithin, NO_SELECTION, targetPages, type PageSelection } from "./selection";
  import { closePanel, fitPanels, inspectorFloats, openPanel, togglePanel, type SidePanel } from "./sidePanels";
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
  import { toggleSidebar, toggleSidebarMode, type SidebarMode, type SidebarState } from "./sidebar";
  import { hiddenParts } from "./toolbar";
  import type { ToolName } from "./tools";
  import ViewerToolbar from "./ViewerToolbar.svelte";

  interface Props {
    document: DocumentState;
    /** the shell answered a change with this new state */
    onDocumentChange: (next: DocumentState) => void;
    /** a change failed; the message says what to do */
    onError: (message: string) => void;
    onRecents: () => void;
    onNewWindow: () => void;
  }

  let { document: doc, onDocumentChange, onError, onRecents, onNewWindow }: Props = $props();

  let scroller = $state<HTMLDivElement>();
  let zoom = $state(1);
  let pageMode = $state<PageMode>("single");
  let scrollTop = $state(0);
  let viewportWidth = $state(0);
  let viewportHeight = $state(0);
  let windowWidth = $state(window.innerWidth);
  let toolbarHeight = $state(0);
  let nightMode = $state(false);
  // the open side panels, oldest first (see sidePanels.ts)
  let panels = $state<SidePanel[]>([]);
  // the sidebar keeps its mode while closed
  let sidebarMode = $state<SidebarMode>("thumbnails");
  const sidebar = $derived<SidebarState>({ open: panels.includes("sidebar"), mode: sidebarMode });
  const showExplain = $derived(panels.includes("explain"));
  const showInspector = $derived(panels.includes("inspector"));
  let inspectorTab = $state("pages");
  // a tool a menu command asked the inspector to open
  let requestedTool = $state<ToolContext["requested"]>(null);
  let selection = $state<PageSelection>(NO_SELECTION);
  // the crop rectangle on the page being read, while the Crop tool is open
  let cropActive = $state(false);
  let cropBox = $state<CropBox | null>(null);
  // a short message under the toolbar, such as "Saved pages 1–3 as …"
  let notice = $state<{ text: string; action?: { label: string; run: () => void } } | null>(null);
  let contextMenuAt = $state<{ x: number; y: number } | null>(null);
  let outline = $state<OutlineItem[] | null>(null);
  let findOpen = $state(false);
  let findBar = $state<FindBar>();
  let found = $state<{ matches: SearchMatch[]; current: number }>({ matches: [], current: -1 });
  // true until the reader picks their own zoom. while it is true the
  // page keeps filling the width, even when the window is resized.
  let followsWidth = true;
  // the revision the outline was read at; it is read again after an edit
  let outlineRevision = -1;
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
  const target = $derived(targetPages(selection, Math.min(currentIndex, doc.pages.length - 1)));
  const floatingInspector = $derived(inspectorFloats(windowWidth, remPx));

  // pages within half a screen of the visible area are kept ready
  const shownPages = $derived.by(() => {
    const range = visiblePages(layout.pages, scrollTop, viewportHeight, viewportHeight / 2);
    return range ? layout.pages.slice(range.first, range.last + 1) : [];
  });

  // the explain panel describes what is on screen right now, with the
  // open inspector tab's tools first
  const onScreen = $derived<ToolName[]>([
    "sidebar",
    "thumbnails",
    ...(sidebar.open && sidebar.mode === "thumbnails" ? (["pageSelection", "movePages", "deletePages"] as const) : []),
    "outline",
    "recents",
    "documentMenu",
    "save",
    "saveAs",
    "undo",
    "redo",
    "rotateLeft",
    "rotateRight",
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
    "tools",
    "explain",
  ]);
  const explained = $derived(explainGroups(onScreen, showInspector ? tabById(inspectorTab) : null));

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

  // panels that don't fit beside each other take turns (see sidePanels.ts)
  function setSidebar(next: SidebarState) {
    sidebarMode = next.mode;
    panels = next.open ? openPanel(panels, "sidebar", windowWidth, remPx) : closePanel(panels, "sidebar");
  }

  function toggleExplain() {
    panels = togglePanel(panels, "explain", windowWidth, remPx);
  }

  function toggleInspector() {
    panels = togglePanel(panels, "inspector", windowWidth, remPx);
  }

  /** opens the inspector at the tab and section holding `tool`. */
  function openTool(tool: ToolName) {
    const tab = tabWithTool(tool);
    if (tab) inspectorTab = tab.id;
    panels = openPanel(panels, "inspector", windowWidth, remPx);
    requestedTool = { tool, at: performance.now() };
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

  /**
   * runs one change to the document (an edit, undo or redo) and shows
   * the result.
   *
   * whether the command can run is checked when its turn comes, not
   * when the key was pressed, so pressing undo twice with one step left
   * undoes once and then quietly does nothing, as on a mac.
   */
  function change(command: Command | null, action: () => Promise<DocumentState>): Promise<boolean> {
    // changes run one after another, so their answers arrive in order
    return documentChanges(async () => {
      if (command && !isAvailable(command, doc)) return false;
      try {
        await showState(await action());
        return true;
      } catch (problem) {
        onError(asCommandError(problem).message);
        return false;
      }
    });
  }

  /**
   * shows a new state of the document: new page sizes (a turned page is
   * wider than tall), new tiles, and the reader still at the same spot
   * of the same page when that page still exists.
   */
  async function showState(next: DocumentState) {
    const anchor = anchorAt(layout.pages, scrollTop + toolbarHeight);
    onDocumentChange(next);
    selection = keepWithin(selection, next.pages.length);
    await tick();
    if (followsWidth) zoom = zoomToFitWidth(next.pages, viewportWidth, pageMode);
    await tick();
    if (scroller) {
      scroller.scrollTop = positionOf(layout.pages, placeAfterEdit(anchor, next.pages.length)) - toolbarHeight;
    }
  }

  // --- page actions shared by the keyboard, the menus and the thumbnails.
  // the pages are chosen when the person asks, not when the change runs

  function rotateTarget(direction: "left" | "right") {
    const command = direction === "left" ? "rotate-left" : "rotate-right";
    const pages = target.pages;
    change(command, () => rotatePages(doc.id, pages, direction === "left" ? -1 : 1));
  }

  /** deletes pages and forgets the selection once they are gone. */
  async function deletePageList(pages: number[]) {
    if (await change(null, () => deletePages(doc.id, pages))) selection = NO_SELECTION;
  }

  /** the delete key deletes selected pages only, never the page being read by surprise. */
  function deleteSelection() {
    if (selection.pages.length > 0) deletePageList(selection.pages);
  }

  /** moves pages as a block, keeps them selected, and follows them if they were being read. */
  async function movePageBlock(pages: number[], before: number) {
    const followed = pages.includes(currentIndex);
    const landed = positionsAfterMove(doc.pages.length, pages, before);
    if (!(await change(null, () => movePages(doc.id, pages, before)))) return;
    selection = { pages: landed, anchor: landed[0], focus: landed[landed.length - 1] };
    if (followed) goToPage(landed[0]);
  }

  function moveByStep(direction: "up" | "down") {
    const pages = selection.pages.length > 0 ? selection.pages : [currentIndex];
    const before = stepTarget(pages, direction, doc.pages.length);
    if (before !== null) movePageBlock(pages, before);
  }

  /** where new pages go: after the last selected page, or after the page being read. */
  const insertAfterTarget = () => Math.max(...target.pages) + 1;

  function insertBlankAfter() {
    const at = insertAfterTarget();
    // the same size as the page before it, so it fits in
    const size = doc.pages[at - 1] ?? { width: 612, height: 792 };
    change(null, () => insertBlankPages(doc.id, { atIndex: at, widthPt: size.width, heightPt: size.height, count: 1 }));
  }

  async function insertFromPdf() {
    const at = insertAfterTarget();
    const path = await chooseFile();
    if (path) insertFiles([path], at);
  }

  /** inserts whole pdfs one after another, each as its own undo step. */
  async function insertFiles(paths: string[], atIndex: number) {
    let at = atIndex;
    for (const path of paths) {
      let added: number;
      try {
        added = await pdfPageCount(path);
      } catch (problem) {
        onError(asCommandError(problem).message);
        return;
      }
      const where = at;
      if (!(await change(null, () => insertPagesFrom(doc.id, where, path, null)))) return;
      at += added;
    }
    selection = NO_SELECTION;
  }

  async function extractTarget() {
    const pages = target.pages;
    const suggested = pathBeside(doc.path, `${stemOf(doc.path)} - ${pagesAsTypedText(pages)}.pdf`);
    const savePath = await chooseSaveLocation(suggested);
    if (!savePath) return;
    try {
      const written = await extractPages(doc.id, pages, savePath);
      showNotice(`Saved ${describePages(pages, doc.pages.length)} as “${fileNameOf(written)}”.`, {
        label: "Open",
        run: () => newWindow(written),
      });
    } catch (problem) {
      onError(asCommandError(problem).message);
    }
  }

  function showNotice(text: string, action?: { label: string; run: () => void }) {
    notice = { text, action };
  }

  /** the notice's button: the message goes away and its action runs. */
  function runNoticeAction() {
    const action = notice?.action;
    notice = null;
    action?.run();
  }

  /** the right-click menu of the thumbnails. */
  const contextItems = $derived.by((): ContextItem[] => {
    const everyPage = target.pages.length >= doc.pages.length;
    const pages = target.pages;
    return [
      { label: "Rotate Left", run: () => rotateTarget("left") },
      { label: "Rotate Right", run: () => rotateTarget("right") },
      {
        label: pages.length === 1 ? "Delete Page" : "Delete Pages",
        disabled: everyPage,
        title: everyPage ? "A PDF needs at least one page" : undefined,
        separatorBefore: true,
        run: () => deletePageList(pages),
      },
      { label: "Extract…", run: extractTarget },
      { label: "Insert Blank Page After", separatorBefore: true, run: insertBlankAfter },
      { label: "Insert from PDF…", run: insertFromPdf },
    ];
  });

  // what every inspector tool can see and do (see inspector.ts)
  const toolContext = $derived<ToolContext>({
    document: doc,
    currentPage: Math.min(currentIndex, doc.pages.length - 1),
    target,
    change: (action) => change(null, action),
    notify: showNotice,
    showError: onError,
    requested: requestedTool,
    crop: {
      box: cropBox,
      setBox: (box) => (cropBox = box),
      setActive: (active) => (cropActive = active),
    },
  });

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
      ["undo", () => change("undo", () => undo(doc.id))],
      ["redo", () => change("redo", () => redo(doc.id))],
      ["rotate-left", () => rotateTarget("left")],
      ["rotate-right", () => rotateTarget("right")],
      ["delete-pages", deleteSelection],
      ["move-pages-up", () => moveByStep("up")],
      ["move-pages-down", () => moveByStep("down")],
      ["insert-blank-page", insertBlankAfter],
      ["insert-from-pdf", insertFromPdf],
      ["extract-pages", extractTarget],
      ["split", () => openTool("splitDocument")],
      ["merge", () => openTool("mergeDocuments")],
      ["crop", () => openTool("cropPages")],
      ["toggle-inspector", toggleInspector],
    ];
    const takeBack = handlers.map(([command, handler]) => appCommands.register(command, handler));
    return () => takeBack.forEach((unregister) => unregister());
  });

  // the outline is read the first time it is shown, then kept until an
  // edit changes the document, when it is read again the next time it shows
  $effect(() => {
    const revision = doc.revision;
    if (!sidebar.open || sidebar.mode !== "outline" || outlineRevision === revision) return;
    outlineRevision = revision;
    untrack(() =>
      documentOutline(doc.id)
        .then((items) => (outline = items))
        // a document whose outline cannot be read is treated as having none
        .catch(() => (outline = [])),
    );
  });

  // the crop rectangle starts fresh on each page it is shown on, and
  // after every change (a crop changes the page's size)
  $effect(() => {
    const size = doc.pages[currentIndex];
    if (!cropActive || !size) {
      cropBox = null;
      return;
    }
    untrack(() => (cropBox = startingCrop(size)));
  });

  // panels that no longer fit when the window shrinks close, oldest first
  $effect(() => {
    const width = windowWidth;
    untrack(() => {
      const fitted = fitPanels(panels, width, remPx);
      if (fitted.length !== panels.length) panels = fitted;
    });
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
      revision={doc.revision}
      {outline}
      onModeChange={(mode: SidebarMode) => setSidebar({ open: true, mode })}
      onGoToPage={goToPage}
      pageHandling={{
        selection,
        onSelectionChange: (next) => (selection = next),
        onMovePages: movePageBlock,
        onContextMenu: (x, y) => (contextMenuAt = { x, y }),
        onInsertFiles: insertFiles,
      }}
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
            revision={doc.revision}
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
          {#if cropBox && page.index === currentIndex}
            <CropOverlay
              {page}
              size={doc.pages[page.index]}
              left={pageLeft(page.left)}
              {zoom}
              box={cropBox}
              onChange={(box) => (cropBox = box)}
            />
          {/if}
        {/each}
      </div>
    </div>

    <ViewerToolbar
      document={doc}
      onDocumentCommand={(command) => appCommands.dispatch(command, "pointer")}
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
      inspectorOpen={showInspector}
      onTools={toggleInspector}
      onExplain={toggleExplain}
    />

    {#if notice}
      <div class="glass notice" role="status" style:top="{toolbarHeight + 8}px">
        <span>{notice.text}</span>
        {#if notice.action}
          <button class="button" onclick={runNoticeAction}>{notice.action.label}</button>
        {/if}
        <button class="button" onclick={() => (notice = null)}>OK</button>
      </div>
    {/if}

    {#if findOpen}
      <FindBar
        bind:this={findBar}
        documentId={doc.id}
        currentPage={currentIndex}
        top={toolbarHeight}
        revision={doc.revision}
        onChange={onFindChange}
        onClose={closeFind}
      />
    {/if}
  </div>

  {#if showInspector}
    <InspectorPanel
      tabId={inspectorTab}
      context={toolContext}
      floating={floatingInspector}
      onTabChange={(id) => (inspectorTab = id)}
      onClose={() => (panels = closePanel(panels, "inspector"))}
    />
  {/if}

  {#if showExplain}
    <HelpPanel groups={explained} onClose={() => (panels = closePanel(panels, "explain"))} />
  {/if}
</div>

{#if contextMenuAt}
  <ContextMenu
    x={contextMenuAt.x}
    y={contextMenuAt.y}
    label="Page commands"
    items={contextItems}
    onClose={() => (contextMenuAt = null)}
  />
{/if}

<style>
  .viewer {
    position: relative;
    display: flex;
    height: 100%;
  }

  /* a message under the toolbar, like the window's own messages */
  .notice {
    position: absolute;
    left: 50%;
    z-index: 2;
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 0.75rem;
    max-width: min(34rem, calc(100% - 2rem));
    padding: 0.625rem 0.625rem 0.625rem 1rem;
    transform: translateX(-50%);
    animation: arrive 0.2s var(--ease);
  }

  .notice span {
    flex: 1 1 12rem;
  }

  @keyframes arrive {
    from {
      opacity: 0;
      transform: translate(-50%, -0.5rem);
    }
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

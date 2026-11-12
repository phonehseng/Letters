<script lang="ts">
  // the Compare screen: its own screen in the window, like the viewer.
  // first the two versions are chosen, then the comparison runs in the
  // background with progress and Stop, then the result shows Side by
  // Side (old left, new right, scrolling together) or as an Overlay,
  // with the changes listed in a sidebar. Back returns to where the
  // person came from; both documents are closed in the engine then.
  //
  // the logic lives in the plain modules beside this file (changes.ts,
  // scrollSync.ts, overlay.ts, progress.ts, setup.ts), all unit tested.
  import { getCurrentWindow } from "@tauri-apps/api/window";
  import { onDestroy, tick } from "svelte";
  import { asCommandError } from "../api";
  import { remInPixels } from "../dom";
  import { fileDropTarget, setFileDropTarget } from "../fileDrop";
  import HelpPanel from "../HelpPanel.svelte";
  import { zoomIn, zoomOut, zoomToFitWidth, placePages } from "../layout";
  import { fileNameOf } from "../pageApi";
  import { EXPLAINED_ON_COMPARE, reportOptions, suggestedReportPath } from "./report";
  import { changeAnnouncement, changesByPage, findChange, listRows, orderedChanges, stepChange, type Side } from "./changes";
  import ChangesList from "./ChangesList.svelte";
  import {
    cancelComparison,
    chooseReportLocation,
    closeComparison,
    comparisonResult,
    comparisonStatus,
    DEFAULT_OPTIONS,
    saveReport,
    startComparison,
    type Category,
    type CompareOptions,
    type CompareStarted,
    type ComparisonResult,
    type JobStatus,
    type ReportFormat,
  } from "./compareApi";
  import CompareColumn from "./CompareColumn.svelte";
  import { compareCommands } from "./compareKeys";
  import CompareProgress from "./CompareProgress.svelte";
  import CompareSetupView from "./CompareSetupView.svelte";
  import CompareToolbar, { type CompareMode } from "./CompareToolbar.svelte";
  import OptionsSheet from "./OptionsSheet.svelte";
  import OverlayView from "./OverlayView.svelte";
  import { POLL_MS } from "./progress";
  import SaveReportSheet from "./SaveReportSheet.svelte";
  import { columnLayout, followingScrollTop, overlaySizesOf, scrollTopToShow, slotOfPage, slotsOf } from "./scrollSync";
  import { isReady, withDropped, type CompareSetup } from "./setup";

  interface Props {
    setup: CompareSetup;
    onClose: () => void;
    /** opens a pdf (the saved report) in the viewer */
    onOpenPdf: (path: string) => void;
  }

  let { setup: initialSetup, onClose, onOpenPdf }: Props = $props();

  type Phase = "setup" | "running" | "done";

  // svelte-ignore state_referenced_locally
  let setup = $state<CompareSetup>(initialSetup);
  let options = $state<CompareOptions>({ ...DEFAULT_OPTIONS });
  let phase = $state<Phase>("setup");
  let started = $state<CompareStarted | null>(null);
  let status = $state<JobStatus>({ state: "running", stage: null, done: 0, total: 0 });
  let result = $state<ComparisonResult | null>(null);
  let mode = $state<CompareMode>("sideBySide");
  let zoom = $state(1);
  let followsWidth = true;
  let linked = $state(true);
  let listOpen = $state(true);
  let explainOpen = $state(false);
  let shown = $state(new Set<Category>(["text", "formatting", "layout", "image", "page"]));
  let selectedId = $state<number | null>(null);
  let sheet = $state<"options" | "report" | null>(null);
  let notice = $state<{ text: string; path?: string } | null>(null);
  let announcement = $state("");
  let toolbarHeight = $state(0);
  let stageWidth = $state(0);
  let windowWidth = $state(window.innerWidth);
  let oldScroller = $state<HTMLDivElement>();
  let newScroller = $state<HTMLDivElement>();
  let overlayScroller = $state<HTMLDivElement>();
  let returnFocus: HTMLElement | null = null;
  const pixelRatio = window.devicePixelRatio || 1;
  // a programmed scroll of one column is not a person scrolling it
  const expected: Record<Side, number | null> = { old: null, new: null };

  const pairs = $derived(result?.pagePairs ?? []);
  const oldSlots = $derived(slotsOf(pairs, "old"));
  const newSlots = $derived(slotsOf(pairs, "new"));
  const overlaySizes = $derived(overlaySizesOf(pairs));
  const oldLayout = $derived(columnLayout(oldSlots, zoom, toolbarHeight));
  const newLayout = $derived(columnLayout(newSlots, zoom, toolbarHeight));
  const overlayLayout = $derived(placePages(overlaySizes, zoom, toolbarHeight, "single"));
  const rows = $derived(result ? listRows(result, shown) : []);
  const ordered = $derived(result ? orderedChanges(result, shown) : []);
  const oldChanges = $derived(result ? changesByPage(result, "old", shown) : new Map());
  const newChanges = $derived(result ? changesByPage(result, "new", shown) : new Map());
  const selected = $derived(result ? findChange(result, selectedId) : null);
  const remPx = remInPixels();
  const narrow = $derived(windowWidth < 52 * remPx);
  const sidePanelOpen = $derived(phase === "done" && (listOpen || explainOpen));
  const title = $derived(started ? `${started.old.name} and ${started.new.name}` : "Compare");
  const columnWidth = $derived(mode === "sideBySide" ? stageWidth / 2 : stageWidth);
  const shownSizes = $derived(mode === "sideBySide" ? [...oldSlots, ...newSlots].map((slot) => slot.size) : overlaySizes);
  const fitZoom = $derived(zoomToFitWidth(shownSizes, columnWidth));

  // the zoom follows the width until the person picks their own
  $effect(() => {
    if (followsWidth && columnWidth > 0 && pairs.length > 0) zoom = fitZoom;
  });

  // the window is named after what it compares
  $effect(() => {
    getCurrentWindow().setTitle(started ? `Compare: ${title}` : "Compare");
  });

  // pdfs dropped on this screen are for it: while choosing they fill the
  // versions, afterwards they are ignored instead of opening behind it
  $effect(() => {
    // the screen underneath (the thumbnails) gets its own target back afterwards
    const previous = fileDropTarget();
    const takeBack = setFileDropTarget({
      over: () => phase === "setup",
      drop: (paths) => {
        if (phase === "setup") setup = withDropped(setup, paths);
        return true;
      },
      leave: () => {},
    });
    return () => {
      takeBack();
      if (previous) setFileDropTarget(previous);
    };
  });

  async function start() {
    if (!isReady(setup)) return;
    await closeJob();
    phase = "running";
    status = { state: "running", stage: null, done: 0, total: 0 };
    result = null;
    selectedId = null;
    try {
      started = await startComparison(setup.older.input, setup.newer.input, $state.snapshot(options));
    } catch (problem) {
      phase = "setup";
      notice = { text: asCommandError(problem).message };
      return;
    }
    await follow(started.jobId);
  }

  /** asks how far the job has got until it ends. */
  async function follow(jobId: number) {
    while (started?.jobId === jobId) {
      try {
        status = await comparisonStatus(jobId);
      } catch (problem) {
        status = { state: "failed", message: asCommandError(problem).message };
      }
      if (status.state !== "running") break;
      await new Promise((resolve) => setTimeout(resolve, POLL_MS));
    }
    if (started?.jobId !== jobId) return;
    if (status.state === "finished") {
      result = await comparisonResult(jobId);
      phase = "done";
      followsWidth = true;
      announce(result.summary.total === 0 ? "These two documents are the same." : `Comparison finished. ${result.summary.total} changes.`);
      return;
    }
    phase = "setup";
    notice = { text: status.state === "failed" ? status.message : "The comparison was stopped." };
    await closeJob();
  }

  async function stop() {
    if (started) await cancelComparison(started.jobId);
  }

  async function closeJob() {
    const job = started;
    started = null;
    if (job) await closeComparison(job.jobId).catch(() => {});
  }

  onDestroy(() => {
    if (started) {
      void cancelComparison(started.jobId).catch(() => {});
      void closeComparison(started.jobId).catch(() => {});
    }
  });

  function announce(text: string) {
    announcement = "";
    tick().then(() => (announcement = text));
  }

  // --- scrolling the two columns together

  function onColumnScroll(side: Side, top: number) {
    const waiting = expected[side];
    if (waiting !== null && Math.abs(waiting - top) < 2) {
      expected[side] = null;
      return;
    }
    expected[side] = null;
    if (!linked) return;
    const other: Side = side === "old" ? "new" : "old";
    const target = other === "old" ? oldScroller : newScroller;
    if (!target) return;
    const next = followingScrollTop(
      pairs,
      side,
      side === "old" ? oldLayout : newLayout,
      other === "old" ? oldLayout : newLayout,
      side === "old" ? oldSlots : newSlots,
      other === "old" ? oldSlots : newSlots,
      top,
      toolbarHeight,
      zoom,
    );
    scrollColumn(other, next);
  }

  function scrollColumn(side: Side, top: number) {
    const element = side === "old" ? oldScroller : newScroller;
    if (!element) return;
    const limited = Math.min(Math.max(0, top), Math.max(0, element.scrollHeight - element.clientHeight));
    if (Math.abs(element.scrollTop - limited) < 1) return;
    expected[side] = limited;
    element.scrollTop = limited;
  }

  // --- picking a change

  /** picks a change and brings it into view on both sides (or in the overlay). */
  function select(id: number, from: Side | "list" = "list") {
    if (!result) return;
    const found = findChange(result, id);
    if (!found) return;
    selectedId = id;
    revealChange(found.pair, found.change, from);
    announce(changeAnnouncement(ordered, found.change, result.pagePairs[found.pair]));
  }

  function revealChange(pair: number, change: NonNullable<typeof selected>["change"], from: Side | "list") {
    if (mode === "overlay") {
      const placed = overlayLayout.pages[pair];
      const rect = change.rightRects[0] ?? change.leftRects[0];
      if (overlayScroller && placed) overlayScroller.scrollTop = scrollTopToShow(overlayLayout, pair, rect?.y ?? 0, zoom, toolbarHeight, overlayScroller.clientHeight);
      return;
    }
    for (const side of ["old", "new"] as Side[]) {
      if (side === from) continue;
      const slots = side === "old" ? oldSlots : newSlots;
      const page = side === "old" ? change.leftPage : change.rightPage;
      const rects = side === "old" ? change.leftRects : change.rightRects;
      const slot = page === null ? pair : slotOfPage(slots, page);
      const element = side === "old" ? oldScroller : newScroller;
      if (slot < 0 || !element) continue;
      const layout = side === "old" ? oldLayout : newLayout;
      scrollColumn(side, scrollTopToShow(layout, slot, rects[0]?.y ?? 0, zoom, toolbarHeight, element.clientHeight));
    }
  }

  function step(direction: "next" | "previous") {
    const id = stepChange(ordered, selectedId, direction);
    if (id !== null) select(id);
  }

  function setFilter(category: Category, on: boolean) {
    const next = new Set(shown);
    if (on) next.add(category);
    else next.delete(category);
    shown = next;
    if (selected && !next.has(selected.change.category)) selectedId = null;
  }

  // --- zoom, keeping the same place on screen

  async function setZoom(next: number) {
    followsWidth = false;
    zoom = next;
    await tick();
  }

  // --- sheets

  function openSheet(which: "options" | "report") {
    returnFocus = document.activeElement as HTMLElement | null;
    sheet = which;
  }

  function closeSheet() {
    sheet = null;
    tick().then(() => returnFocus?.focus());
  }

  async function compareAgain(next: CompareOptions) {
    options = next;
    closeSheet();
    await start();
  }

  async function saveReportAs(format: ReportFormat, thumbnails: boolean) {
    closeSheet();
    if (!started) return;
    const newerPath = setup.newer?.input.kind === "file" ? setup.newer.input.path : null;
    const path = await chooseReportLocation(suggestedReportPath(newerPath, format), format);
    if (!path) return;
    const words = reportOptions(started.old.name, started.new.name, thumbnails, new Date(), navigator.language);
    try {
      const written = await saveReport(started.jobId, path, format, words);
      notice = { text: `Saved the report as “${fileNameOf(written)}”.`, path: format === "pdf" ? written : undefined };
    } catch (problem) {
      notice = { text: asCommandError(problem).message };
    }
  }

  // the screen's commands, while it shows (see compareKeys.ts)
  $effect(() => {
    const handlers: [Parameters<typeof compareCommands.register>[0], () => void][] = [
      ["zoom-in", () => setZoom(zoomIn(zoom))],
      ["zoom-out", () => setZoom(zoomOut(zoom))],
      ["fit-width", () => (followsWidth = true)],
      ["next-change", () => step("next")],
      ["previous-change", () => step("previous")],
      // alt with the arrows, the keys that move pages in the viewer
      ["move-pages-down", () => step("next")],
      ["move-pages-up", () => step("previous")],
    ];
    const takeBack = handlers.map(([command, handler]) => compareCommands.register(command, handler));
    return () => takeBack.forEach((unregister) => unregister());
  });

  function onKeyDown(event: KeyboardEvent) {
    if (event.key === "Escape" && !sheet && !event.defaultPrevented && selectedId !== null) {
      selectedId = null;
      event.preventDefault();
    }
  }
</script>

<svelte:window bind:innerWidth={windowWidth} onkeydown={onKeyDown} />

<div class="compare">
  <main class="stage" bind:clientWidth={stageWidth}>
    {#if phase === "setup"}
      <div class="scroll">
        <CompareSetupView {setup} {options} onSetup={(next) => (setup = next)} onOptions={(next) => (options = next)} onStart={start} />
      </div>
    {:else if phase === "running"}
      <CompareProgress {status} names={{ older: setup.older?.name ?? "", newer: setup.newer?.name ?? "" }} onStop={stop} />
    {:else if result && started}
      {#if mode === "sideBySide"}
        <div class="columns">
          <CompareColumn
            side="old"
            label={`Old version: ${started.old.name}`}
            documentId={started.old.documentId}
            slots={oldSlots}
            layout={oldLayout}
            {zoom}
            {pixelRatio}
            changesOnPage={oldChanges}
            {selectedId}
            inset={toolbarHeight}
            onSelect={(id) => select(id, "old")}
            onScroll={(top) => onColumnScroll("old", top)}
            bind:scroller={oldScroller}
          />
          <div class="divider" aria-hidden="true"></div>
          <CompareColumn
            side="new"
            label={`New version: ${started.new.name}`}
            documentId={started.new.documentId}
            slots={newSlots}
            layout={newLayout}
            {zoom}
            {pixelRatio}
            changesOnPage={newChanges}
            {selectedId}
            inset={toolbarHeight}
            onSelect={(id) => select(id, "new")}
            onScroll={(top) => onColumnScroll("new", top)}
            bind:scroller={newScroller}
          />
        </div>
      {:else}
        <OverlayView
          jobId={started.jobId}
          {pairs}
          layout={overlayLayout}
          {zoom}
          {pixelRatio}
          inset={toolbarHeight}
          {selected}
          bind:scroller={overlayScroller}
        />
      {/if}
    {/if}

    <CompareToolbar
      {title}
      showResults={phase === "done"}
      {mode}
      {zoom}
      {linked}
      {listOpen}
      {explainOpen}
      {narrow}
      bind:height={toolbarHeight}
      onBack={onClose}
      onMode={(next) => {
        mode = next;
        followsWidth = true;
      }}
      onZoomOut={() => setZoom(zoomOut(zoom))}
      onZoomIn={() => setZoom(zoomIn(zoom))}
      onFitWidth={() => (followsWidth = true)}
      onLink={() => (linked = !linked)}
      onList={() => {
        listOpen = !listOpen;
        if (listOpen) explainOpen = false;
      }}
      onOptions={() => openSheet("options")}
      onSaveReport={() => openSheet("report")}
      onExplain={() => {
        explainOpen = !explainOpen;
        if (explainOpen) listOpen = false;
      }}
    />

    {#if notice}
      <div class="glass notice" role="status" style:top="{toolbarHeight + 8}px">
        <span>{notice.text}</span>
        {#if notice.path}
          {@const path = notice.path}
          <button class="button" onclick={() => onOpenPdf(path)}>Open</button>
        {/if}
        <button class="button" onclick={() => (notice = null)}>OK</button>
      </div>
    {/if}
    <div class="visually-hidden" role="status" aria-live="polite">{announcement}</div>
  </main>

  {#if sidePanelOpen && result}
    {#if explainOpen}
      <HelpPanel groups={[{ title: "On this screen", names: [...EXPLAINED_ON_COMPARE] }]} onClose={() => (explainOpen = false)} />
    {:else}
      <ChangesList {result} {rows} {ordered} {shown} {selectedId} onFilter={setFilter} onSelect={(id) => select(id)} onStep={step} />
    {/if}
  {/if}
</div>

{#if sheet === "options"}
  <OptionsSheet {options} onCompare={compareAgain} onCancel={closeSheet} />
{:else if sheet === "report"}
  <SaveReportSheet onSave={saveReportAs} onCancel={closeSheet} />
{/if}

<style>
  .compare {
    display: flex;
    height: 100%;
    background: var(--desk);
  }

  .stage {
    position: relative;
    flex: 1;
    min-width: 0;
  }

  .scroll {
    height: 100%;
    overflow-y: auto;
    background: var(--window);
  }

  .columns {
    display: flex;
    height: 100%;
  }

  .divider {
    width: 0.5px;
    background: var(--separator);
  }

  .notice {
    position: absolute;
    left: 50%;
    z-index: 3;
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 0.75rem;
    max-width: min(34rem, calc(100% - 2rem));
    padding: 0.625rem 0.625rem 0.625rem 1rem;
    transform: translateX(-50%);
  }

  .notice span {
    flex: 1 1 12rem;
  }
</style>

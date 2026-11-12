<script lang="ts">
  // the viewer's floating toolbar, in three areas with fixed jobs:
  // where you are (leading), how big you see it (centre), and what else
  // you can do (trailing). see docs/DESIGN.md.
  //
  // on a narrow window parts give way in the order set in toolbar.ts;
  // everything hidden keeps its keyboard shortcut.
  import type { DocumentState } from "./api";
  import type { Command } from "./commands";
  import DocumentMenu from "./DocumentMenu.svelte";
  import ToolButton from "./ToolButton.svelte";
  import type { ToolbarPart } from "./toolbar";
  import { tooltipFor, tools } from "./tools";
  import ViewMenu from "./ViewMenu.svelte";

  interface Props {
    document: DocumentState;
    /** a command picked in the document menu */
    onDocumentCommand: (command: Command) => void;
    /** counted from 1, as people count pages */
    currentPage: number;
    zoom: number;
    hidden: Set<ToolbarPart>;
    sidebarOpen: boolean;
    findOpen: boolean;
    explainOpen: boolean;
    twoPages: boolean;
    nightMode: boolean;
    height?: number;
    onSidebar: () => void;
    onRecents: () => void;
    onGoToPage: (pageNumber: number) => void;
    onZoomOut: () => void;
    onZoomIn: () => void;
    onFitWidth: () => void;
    onFind: () => void;
    onToggleTwoPages: () => void;
    onToggleNightMode: () => void;
    onNewWindow: () => void;
    inspectorOpen: boolean;
    onTools: () => void;
    onExplain: () => void;
    /** ai tools: the button shows only while they are on in Settings (lib/ai) */
    aiTools?: { open: boolean; onToggle: () => void } | null;
  }

  let {
    document: doc,
    onDocumentCommand,
    currentPage,
    zoom,
    hidden,
    sidebarOpen,
    findOpen,
    explainOpen,
    twoPages,
    nightMode,
    height = $bindable(0),
    onSidebar,
    onRecents,
    onGoToPage,
    onZoomOut,
    onZoomIn,
    onFitWidth,
    onFind,
    onToggleTwoPages,
    onToggleNightMode,
    onNewWindow,
    inspectorOpen,
    onTools,
    onExplain,
    aiTools = null,
  }: Props = $props();

  const hideLabel = $derived(hidden.has("labels"));
  const pageCount = $derived(doc.pages.length);

  function onPageInput(event: KeyboardEvent) {
    if (event.key !== "Enter") return;
    onGoToPage(Number((event.currentTarget as HTMLInputElement).value));
  }
</script>

<div
  class="toolbar"
  class:uncentred={hidden.has("zoomLevel")}
  role="toolbar"
  aria-label="Document tools"
  bind:clientHeight={height}
>
  <div class="area leading">
    <div class="glass group">
      <ToolButton name="sidebar" icon="sidebar" {hideLabel} pressed={sidebarOpen} onClick={onSidebar} />
      <ToolButton name="recents" icon="back" symbolOnly onClick={onRecents} />
      <div class="title">
        <DocumentMenu document={doc} onCommand={onDocumentCommand} />
        <label class="page-jump" title={tooltipFor("goToPage")}>
          <span class:visually-hidden={hidden.has("pageWord")}>{tools.goToPage.label}</span>
          <input type="number" min="1" max={pageCount} value={currentPage} onkeydown={onPageInput} />
          <span>of {pageCount}</span>
        </label>
      </div>
    </div>
  </div>

  <!-- the centre area stays even when empty, so the grid keeps its three columns -->
  <div class="area">
    {#if !hidden.has("zoomButtons")}
      <div class="glass group">
        <ToolButton name="zoomOut" icon="zoomOut" {hideLabel} onClick={onZoomOut} />
        {#if !hidden.has("zoomLevel")}
          <span class="zoom-level" aria-live="polite" title="How big the page is shown">
            {Math.round(zoom * 100)}%
          </span>
        {/if}
        <ToolButton name="zoomIn" icon="zoomIn" {hideLabel} onClick={onZoomIn} />
        {#if !hidden.has("fitWidth")}
          <ToolButton name="fitWidth" icon="fitWidth" {hideLabel} onClick={onFitWidth} />
        {/if}
      </div>
    {/if}
  </div>

  <div class="area trailing">
    <div class="glass group">
      {#if !hidden.has("find")}
        <ToolButton name="find" icon="search" {hideLabel} pressed={findOpen} onClick={onFind} />
      {/if}
      <ViewMenu {twoPages} {nightMode} {onToggleTwoPages} {onToggleNightMode} {hideLabel} />
      {#if !hidden.has("newWindow")}
        <ToolButton name="newWindow" icon="newWindow" {hideLabel} onClick={onNewWindow} />
      {/if}
      <!-- inspector buttons sit on the trailing edge, beside the panels they open -->
      <ToolButton name="tools" icon="tools" {hideLabel} pressed={inspectorOpen} onClick={onTools} />
      {#if aiTools}
        <ToolButton name="aiTools" icon="sidePanel" {hideLabel} pressed={aiTools.open} onClick={aiTools.onToggle} />
      {/if}
      <ToolButton name="explain" icon="sidePanel" {hideLabel} pressed={explainOpen} onClick={onExplain} />
    </div>
  </div>
</div>

<style>
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

  /* on a narrow window the zoom group stops being centred, so the
     title gets the space it would have kept free on its right */
  .toolbar.uncentred {
    grid-template-columns: minmax(0, 1fr) auto auto;
    gap: 0.5rem;
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
    /* the title gives way first when space runs out. the padding keeps
       the page field's focus ring inside the clipped area */
    padding: 0.25rem 0.75rem 0.25rem 0.375rem;
    overflow: hidden;
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
</style>

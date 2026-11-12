<script lang="ts">
  // the Compare screen's floating toolbar, laid out like the viewer's:
  // leading Back and the two names, centre the view (Side by Side or
  // Overlay) and zoom, trailing scrolling, the changes list, options, the
  // report and Explain.
  import ToolButton from "../ToolButton.svelte";
  import { nextIndex } from "../keyboard";
  import { tooltipFor } from "../tools";

  export type CompareMode = "sideBySide" | "overlay";

  interface Props {
    title: string;
    showResults: boolean;
    mode: CompareMode;
    zoom: number;
    linked: boolean;
    listOpen: boolean;
    explainOpen: boolean;
    narrow: boolean;
    height?: number;
    onBack: () => void;
    onMode: (mode: CompareMode) => void;
    onZoomOut: () => void;
    onZoomIn: () => void;
    onFitWidth: () => void;
    onLink: () => void;
    onList: () => void;
    onOptions: () => void;
    onSaveReport: () => void;
    onExplain: () => void;
  }

  let {
    title,
    showResults,
    mode,
    zoom,
    linked,
    listOpen,
    explainOpen,
    narrow,
    height = $bindable(0),
    onBack,
    onMode,
    onZoomOut,
    onZoomIn,
    onFitWidth,
    onLink,
    onList,
    onOptions,
    onSaveReport,
    onExplain,
  }: Props = $props();

  const MODES: { mode: CompareMode; label: string; tool: "compareSideBySide" | "compareOverlay" }[] = [
    { mode: "sideBySide", label: "Side by Side", tool: "compareSideBySide" },
    { mode: "overlay", label: "Overlay", tool: "compareOverlay" },
  ];
  let modeButtons: HTMLButtonElement[] = $state([]);

  function onModeKey(event: KeyboardEvent) {
    const current = MODES.findIndex((entry) => entry.mode === mode);
    const next = nextIndex(current, event.key, MODES.length, { orientation: "horizontal", wrap: true });
    if (next === null) return;
    event.preventDefault();
    onMode(MODES[next].mode);
    modeButtons[next]?.focus();
  }
</script>

<div class="toolbar" role="toolbar" aria-label="Compare tools" bind:clientHeight={height}>
  <div class="area leading">
    <div class="glass group">
      <ToolButton name="compareBack" icon="back" symbolOnly onClick={onBack} />
      <h1 class="title">{title}</h1>
    </div>
  </div>

  <div class="area">
    {#if showResults}
      <div class="glass group">
        <div class="segments" role="radiogroup" aria-label="View">
          {#each MODES as entry, index (entry.mode)}
            <button
              class="segment"
              class:selected={mode === entry.mode}
              role="radio"
              aria-checked={mode === entry.mode}
              tabindex={mode === entry.mode ? 0 : -1}
              title={tooltipFor(entry.tool)}
              bind:this={modeButtons[index]}
              onclick={() => onMode(entry.mode)}
              onkeydown={onModeKey}
            >
              {entry.label}
            </button>
          {/each}
        </div>
        {#if !narrow}
          <ToolButton name="zoomOut" icon="zoomOut" hideLabel onClick={onZoomOut} />
          <span class="zoom-level" aria-live="polite" title="How big the pages are shown">{Math.round(zoom * 100)}%</span>
          <ToolButton name="zoomIn" icon="zoomIn" hideLabel onClick={onZoomIn} />
          <ToolButton name="fitWidth" icon="fitWidth" hideLabel onClick={onFitWidth} />
        {/if}
      </div>
    {/if}
  </div>

  <div class="area trailing">
    {#if showResults}
      <div class="glass group">
        {#if mode === "sideBySide"}
          <ToolButton name="compareLinkScrolling" icon="twoPages" hideLabel={narrow} pressed={linked} onClick={onLink} />
        {/if}
        <ToolButton name="compareOptions" icon="tools" hideLabel={narrow} onClick={onOptions} />
        <ToolButton name="compareSaveReport" icon="arrowDown" hideLabel={narrow} onClick={onSaveReport} />
        <ToolButton name="compareChanges" icon="sidePanel" hideLabel={narrow} pressed={listOpen} onClick={onList} />
        <ToolButton name="explain" icon="view" hideLabel={narrow} pressed={explainOpen} onClick={onExplain} />
      </div>
    {/if}
  </div>
</div>

<style>
  .toolbar {
    position: absolute;
    inset: 0 0.75rem auto 0;
    z-index: 2;
    display: grid;
    grid-template-columns: minmax(0, 1fr) auto minmax(0, 1fr);
    align-items: start;
    gap: 0.75rem;
    padding: 0.625rem 0 0.625rem 0.75rem;
    pointer-events: none;
  }

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
    min-width: 0;
    margin: 0;
    padding: 0 0.75rem 0 0.25rem;
    overflow: hidden;
    font: var(--headline);
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .segments {
    display: flex;
    margin-right: 0.25rem;
    padding: 0.125rem;
    border-radius: var(--radius-control);
    background: var(--control);
  }

  .segment {
    min-height: 2.25rem;
    padding: 0 0.75rem;
    border: 0;
    border-radius: calc(var(--radius-control) - 0.125rem);
    background: transparent;
    color: var(--label-secondary);
    font: var(--subheadline);
    font-weight: 600;
    white-space: nowrap;
  }

  .segment.selected {
    background: var(--segment-selected);
    box-shadow: var(--shadow-segment);
    color: var(--label);
  }

  .zoom-level {
    min-width: 2.75rem;
    color: var(--label-secondary);
    font: var(--subheadline);
    font-variant-numeric: tabular-nums;
    text-align: center;
  }
</style>

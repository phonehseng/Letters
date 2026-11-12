<script lang="ts">
  // the ruler above the page: the margins in grey, numbers every inch or
  // centimetre from the left margin, and three labelled markers (First
  // Line, Left, Right) that can be dragged or moved with the arrow keys.
  // clicking the strip adds a tab stop; dragging a stop off removes it.
  import type { Indent, TabStop } from "./model";
  import { MARKER_LABELS, addTabStop, markerDescription, markerPositions, moveMarker, moveTabStop, nudge, removeTabStop, snap, ticks, type Marker, type RulerFrame } from "./ruler";
  import { tooltipFor } from "../tools";
  import { formatLength, pxToTwips, type LengthUnit } from "./units";

  interface Props {
    frame: RulerFrame;
    indent: Indent;
    tabs: TabStop[];
    zoom: number;
    unit: LengthUnit;
    onIndent: (indent: Indent) => void;
    onTabs: (tabs: TabStop[]) => void;
  }

  let { frame, indent, tabs, zoom, unit, onIndent, onTabs }: Props = $props();

  let strip = $state<HTMLDivElement>();
  // while dragging, the marker follows the pointer before the paragraph changes
  let dragging = $state<{ marker: Marker; indent: Indent } | null>(null);
  let draggingTab = $state<{ from: number; at: number; off: boolean } | null>(null);

  const shown = $derived(dragging?.indent ?? indent);
  const positions = $derived(markerPositions(frame, shown));
  const scale = $derived(zoom / 15);
  const x = (twips: number) => twips * scale;
  const format = (twips: number) => formatLength(twips, unit);
  const tickList = $derived(ticks(frame, unit));
  const toolName: Record<Marker, "wRulerFirstLine" | "wRulerLeft" | "wRulerRight"> = { firstLine: "wRulerFirstLine", left: "wRulerLeft", right: "wRulerRight" };

  /** twips from the page's left edge under a pointer. */
  function twipsAt(clientX: number): number {
    const rect = strip?.getBoundingClientRect();
    return rect ? pxToTwips((clientX - rect.left) / zoom) : 0;
  }

  function startDrag(marker: Marker, event: PointerEvent) {
    event.preventDefault();
    (event.currentTarget as HTMLElement).setPointerCapture(event.pointerId);
    dragging = { marker, indent };
  }

  function dragMove(event: PointerEvent) {
    if (!dragging) return;
    const at = event.altKey ? twipsAt(event.clientX) : snap(twipsAt(event.clientX), unit);
    dragging = { marker: dragging.marker, indent: moveMarker(frame, indent, dragging.marker, at) };
  }

  function endDrag() {
    if (dragging) onIndent(dragging.indent);
    dragging = null;
  }

  function onMarkerKey(marker: Marker, event: KeyboardEvent) {
    if (event.key !== "ArrowLeft" && event.key !== "ArrowRight") return;
    event.preventDefault();
    onIndent(nudge(frame, indent, marker, event.key === "ArrowLeft" ? -1 : 1, unit, event.shiftKey));
  }

  function onStripClick(event: MouseEvent) {
    if (event.target !== strip) return;
    const fromMargin = snap(twipsAt(event.clientX), unit) - frame.marginLeft;
    if (fromMargin <= 0 || fromMargin + frame.marginLeft >= frame.pageWidth - frame.marginRight) return;
    onTabs(addTabStop(tabs, fromMargin));
  }

  function startTabDrag(stop: TabStop, event: PointerEvent) {
    event.preventDefault();
    event.stopPropagation();
    (event.currentTarget as HTMLElement).setPointerCapture(event.pointerId);
    draggingTab = { from: stop.position, at: stop.position, off: false };
  }

  function tabMove(event: PointerEvent) {
    if (!draggingTab || !strip) return;
    const rect = strip.getBoundingClientRect();
    const off = event.clientY > rect.bottom + 24 || event.clientY < rect.top - 24;
    draggingTab = { ...draggingTab, at: snap(twipsAt(event.clientX), unit) - frame.marginLeft, off };
  }

  function endTabDrag() {
    if (!draggingTab) return;
    const { from, at, off } = draggingTab;
    draggingTab = null;
    onTabs(off ? removeTabStop(tabs, from) : moveTabStop(tabs, from, at));
  }

  function onTabKey(stop: TabStop, event: KeyboardEvent) {
    if (event.key === "Delete" || event.key === "Backspace") {
      event.preventDefault();
      onTabs(removeTabStop(tabs, stop.position));
    } else if (event.key === "ArrowLeft" || event.key === "ArrowRight") {
      event.preventDefault();
      const step = unit === "in" ? 180 : 142;
      onTabs(moveTabStop(tabs, stop.position, stop.position + (event.key === "ArrowLeft" ? -step : step)));
    }
  }
</script>

<div class="ruler" role="group" aria-label="Ruler: indents and tab stops of the paragraph with the cursor" title={tooltipFor("wRuler")}>
  <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
  <div class="strip" bind:this={strip} style:width="{x(frame.pageWidth)}px" onclick={onStripClick}>
    <div class="margin" style:width="{x(frame.marginLeft)}px"></div>
    <div class="margin right" style:width="{x(frame.marginRight)}px"></div>
    {#each tickList as tick (tick.at)}
      <span class="tick {tick.kind}" style:left="{x(frame.marginLeft + tick.at)}px">{tick.label ?? ""}</span>
    {/each}
    {#each tabs as stop (stop.position)}
      <button
        class="tab-stop"
        style:left="{x(frame.marginLeft + (draggingTab?.from === stop.position ? draggingTab.at : stop.position))}px"
        class:leaving={draggingTab?.from === stop.position && draggingTab.off}
        title={`Tab stop at ${format(stop.position)} from the margin. Drag it off the ruler or press Delete to remove it.`}
        aria-label={`Tab stop at ${format(stop.position)}`}
        onpointerdown={(event) => startTabDrag(stop, event)}
        onpointermove={tabMove}
        onpointerup={endTabDrag}
        onkeydown={(event) => onTabKey(stop, event)}
      ></button>
    {/each}
    {#each ["firstLine", "left", "right"] as const as marker (marker)}
      <button
        class="marker {marker}"
        class:active={dragging?.marker === marker}
        style:left="{x(positions[marker])}px"
        role="slider"
        aria-label={MARKER_LABELS[marker]}
        aria-valuemin={0}
        aria-valuemax={frame.pageWidth}
        aria-valuenow={positions[marker]}
        aria-valuetext={markerDescription(marker, frame, shown, format)}
        title={`${markerDescription(marker, frame, shown, format)} ${tooltipFor(toolName[marker]).replace(/^[^(]*/, "").trim()}`}
        onpointerdown={(event) => startDrag(marker, event)}
        onpointermove={dragMove}
        onpointerup={endDrag}
        onpointercancel={() => (dragging = null)}
        onkeydown={(event) => onMarkerKey(marker, event)}
      >
        <span class="shape" aria-hidden="true"></span>
        <span class="label">{MARKER_LABELS[marker]}</span>
      </button>
    {/each}
  </div>
  {#if dragging}
    <div class="readout" role="status" style:left="{x(positions[dragging.marker])}px">
      {MARKER_LABELS[dragging.marker]}: {format(positions[dragging.marker] - frame.marginLeft)}
    </div>
  {/if}
</div>

<style>
  .ruler {
    position: relative;
    display: flex;
    justify-content: center;
    height: 2.5rem;
    user-select: none;
  }

  .strip {
    position: relative;
    height: 1.125rem;
    margin-top: 0.25rem;
    border-radius: 0.25rem;
    background: var(--glass);
    box-shadow: inset 0 0 0 0.5px var(--separator);
    font: var(--footnote);
    letter-spacing: var(--footnote-spacing);
  }

  .margin {
    position: absolute;
    top: 0;
    bottom: 0;
    left: 0;
    border-radius: 0.25rem 0 0 0.25rem;
    background: var(--control-active);
    pointer-events: none;
  }

  .margin.right {
    left: auto;
    right: 0;
    border-radius: 0 0.25rem 0.25rem 0;
  }

  .tick {
    position: absolute;
    bottom: 0;
    width: 1px;
    height: 0.25rem;
    background: var(--label-tertiary);
    pointer-events: none;
  }

  .tick.half {
    height: 0.375rem;
  }

  .tick.number {
    width: auto;
    height: auto;
    top: 0.125rem;
    bottom: auto;
    background: none;
    color: var(--label-secondary);
    transform: translateX(-50%);
  }

  .marker {
    position: absolute;
    top: -0.25rem;
    display: flex;
    flex-direction: column;
    align-items: center;
    width: 0;
    height: 2.25rem;
    padding: 0;
    border: 0;
    background: none;
    color: var(--accent-label);
    touch-action: none;
  }

  .marker .shape {
    width: 0.75rem;
    height: 0.5rem;
    background: var(--accent);
    clip-path: polygon(0 0, 100% 0, 50% 100%);
  }

  /* left and right point up from below the strip */
  .marker.left,
  .marker.right {
    top: 0.75rem;
  }

  .marker.left .shape,
  .marker.right .shape {
    clip-path: polygon(50% 0, 100% 100%, 0 100%);
  }

  .marker .label {
    margin-top: 0.0625rem;
    padding: 0 0.125rem;
    border-radius: 0.1875rem;
    background: var(--window);
    color: var(--label-secondary);
    font: var(--footnote);
    white-space: nowrap;
  }

  /* first line's label sits above its marker, the others below theirs */
  .marker.firstLine {
    flex-direction: column-reverse;
    top: -1.125rem;
    height: auto;
  }

  .marker.right .label {
    transform: translateX(-30%);
  }

  .marker.left .label {
    transform: translateX(30%);
  }

  .marker:focus-visible .shape,
  .marker.active .shape {
    background: var(--accent-label);
    outline: 2px solid var(--accent-soft);
  }

  .tab-stop {
    position: absolute;
    top: 0.5rem;
    width: 0.5rem;
    height: 0.5rem;
    margin-left: -0.0625rem;
    padding: 0;
    border: 0;
    border-left: 2px solid var(--label);
    border-bottom: 2px solid var(--label);
    background: none;
    touch-action: none;
  }

  .tab-stop.leaving {
    opacity: 0.3;
  }

  .readout {
    position: absolute;
    top: 2.375rem;
    padding: 0.125rem 0.375rem;
    border-radius: 0.375rem;
    background: var(--glass);
    box-shadow: var(--shadow-glass);
    font: var(--subheadline);
    transform: translateX(-50%);
    white-space: nowrap;
    z-index: 2;
  }
</style>

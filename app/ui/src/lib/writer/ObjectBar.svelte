<script lang="ts">
  // the small glass bar above a selected picture or text block. for a
  // picture: In Line or Free (the one thing most people want to change),
  // how text wraps a free one, Crop, Lock Proportions, Original Size and
  // Fit Width; for a text block: how text wraps it and Return to Text.
  // both: stacking, lining up (with Shift-clicked others) and exact
  // numbers for size, place and angle. one sentence says what the current
  // state means.
  import { tooltipFor } from "../tools";
  import type { Box } from "./frameMaths";
  import { pageAt, toPageTwips } from "./objectGeometry";
  import { fitWidth } from "./resize";
  import { gate } from "./strictMode";
  import { formatLength, lengthToTwips, lengthValue, type LengthUnit } from "./units";
  import { rootPoint, type SelectedObject } from "./writerObjects.svelte";
  import type { WriterSession } from "./writerSession.svelte";

  interface Props {
    session: WriterSession;
    selected: SelectedObject;
    unit: LengthUnit;
    cropping: boolean;
    natural: { width: number; height: number };
    /** the frame on screen */
    box: Box;
    onCrop: () => void;
    onRotation: (angle: number) => void;
  }

  let { session, selected, unit, cropping, natural, box, onCrop, onRotation }: Props = $props();

  type Data = Record<string, unknown>;
  const data = $derived(selected.node.attrs.data as Data);

  /** a typed length ("2.5", "2,5") in twips, or null. */
  function parseLength(text: string, lengthUnit: LengthUnit): number | null {
    const value = Number.parseFloat(text.replace(",", "."));
    return Number.isFinite(value) ? lengthToTwips(value, lengthUnit) : null;
  }
  const isImage = $derived(selected.kind === "image");
  const wrap = $derived(String((isImage ? (data.placement as Data | undefined)?.wrap : data.wrap) ?? "square"));
  const locked = $derived(data.lockAspectRatio !== false);
  const mode = $derived(session.settings.mode === "strict" ? "strict" : "free");
  const cropGate = $derived(gate("wImageCrop", mode, session.features));
  const picked = $derived(session.objects.picked.length);

  /** where the object sits on its page, in twips, for the X and Y fields. */
  const place = $derived.by(() => {
    const layout = session.objects.layout;
    const view = session.view;
    if (!layout || !view || !selected.free) return null;
    const root = rootPoint(view.dom, box, session.zoom);
    return toPageTwips(root.left, root.top, pageAt(root.top, layout), layout);
  });

  function setWrap(value: string) {
    if (isImage && value === "inline") {
      session.objects.setImageFree(false);
      return;
    }
    if (isImage && !selected.free) {
      session.objects.setImageFree(true);
      return;
    }
    session.objects.setData(selected.pos, (current) => (isImage ? { ...current, placement: { ...(current.placement as Data), wrap: value } } : { ...current, wrap: value }));
  }

  function setSize(side: "width" | "height", text: string) {
    const twips = parseLength(text, unit);
    if (!twips || twips <= 0) return;
    session.objects.setData(selected.pos, (current) => {
      const width = Number(current.width);
      const height = Number(current.height);
      if (!(isImage && locked)) return { ...current, [side]: twips, ...(side === "height" && !isImage ? { autoHeight: false } : {}) };
      return side === "width" ? { ...current, width: twips, height: Math.round((twips * height) / Math.max(1, width)) } : { ...current, height: twips, width: Math.round((twips * width) / Math.max(1, height)) };
    });
  }

  function setPlace(axis: "x" | "y", text: string) {
    const twips = parseLength(text, unit);
    const layout = session.objects.layout;
    const view = session.view;
    if (twips === null || !place || !layout || !view) return;
    const root = rootPoint(view.dom, box, session.zoom);
    const page = pageAt(root.top, layout);
    const edge = layout.pageEdges[page] ?? 0;
    const x = axis === "x" ? twips : place.x;
    const y = axis === "y" ? twips : place.y;
    session.objects.moveTo(selected.pos, x / 15 - layout.margins.left / 15, edge + y / 15);
  }

  function originalSize() {
    if (!natural.width) return;
    session.objects.setData(selected.pos, (current) => ({ ...current, width: natural.width * 15, height: natural.height * 15 }));
  }

  function pageWidth() {
    const page = session.meta.sections?.[0]?.page;
    const limit = page ? page.width - page.margins.left - page.margins.right : 9360;
    session.objects.setData(selected.pos, (current) => {
      const size = { width: Number(current.width), height: Number(current.height) };
      const fitted = size.width < limit ? { width: limit, height: Math.round((limit * size.height) / Math.max(1, size.width)) } : fitWidth(size, limit);
      return { ...current, ...fitted };
    });
  }

  function hint(): string {
    if (cropping) return "Cropping: drag a handle to cut that side, then press Return.";
    if (!isImage) return "A text block: drag its edge to move it. Type in it like any text.";
    if (!selected.free) return "In Line: it moves with the text, like a big letter.";
    if (wrap === "behindText") return "Behind the text: the words run over it.";
    if (wrap === "inFrontOfText") return "In front of the text: it covers the words.";
    if (wrap === "topAndBottom") return "Free: text stops above it and goes on below.";
    return "Free: drag it anywhere. Text flows around it, and Word keeps it where you put it.";
  }

  // the bar sits above the object, clear of its edges and handles; below
  // it when there is no room above
  let barHeight = $state(72);
  const top = $derived(box.top - barHeight - 28 >= 8 ? box.top - barHeight - 28 : box.top + box.height + 16);
</script>

<div class="glass bar" bind:clientHeight={barHeight} style:left="{Math.max(8, box.left)}px" style:top="{top}px" role="toolbar" aria-label={isImage ? "Picture" : "Text block"}>
  <div class="row">
    {#if isImage}
      <div class="segments" role="radiogroup" aria-label="How the picture sits">
        <button class="segment" class:selected={!selected.free} role="radio" aria-checked={!selected.free} title={tooltipFor("wImageInline")} onclick={() => session.objects.setImageFree(false)}>In Line</button>
        <button class="segment" class:selected={selected.free} role="radio" aria-checked={selected.free} title={tooltipFor("wImageFree")} onclick={() => session.objects.setImageFree(true)}>Free</button>
      </div>
    {/if}
    {#if selected.free}
      <select class="field" aria-label="Text around it" title={tooltipFor("wImageWrap")} value={wrap === "tight" || wrap === "through" ? "square" : wrap} onchange={(event) => setWrap(event.currentTarget.value)}>
        <option value="square">Text Around</option>
        <option value="topAndBottom">Above and Below</option>
        <option value="behindText">Behind Text</option>
        <option value="inFrontOfText">In Front of Text</option>
      </select>
      <button class="button" title={tooltipFor("wBringToFront")} onclick={() => session.objects.stack(true)}>Front</button>
      <button class="button" title={tooltipFor("wSendToBack")} onclick={() => session.objects.stack(false)}>Back</button>
    {/if}
    {#if isImage}
      <button class="button" class:pressed={cropping} aria-pressed={cropping} disabled={!cropGate.enabled} title={cropGate.reason ?? tooltipFor("wImageCrop")} onclick={onCrop}>{cropping ? "Done" : "Crop"}</button>
      <button
        class="button"
        class:pressed={locked}
        aria-pressed={locked}
        title={tooltipFor("wImageLock")}
        onclick={() => session.objects.setData(selected.pos, (current) => ({ ...current, lockAspectRatio: !locked }))}>Lock Proportions</button
      >
      <button class="button" title={tooltipFor("wImageOriginalSize")} onclick={originalSize}>Original Size</button>
      <button class="button" title={tooltipFor("wImageFitWidth")} onclick={pageWidth}>Fit Width</button>
    {:else}
      <button
        class="button"
        title={tooltipFor("wTextBlockDissolve")}
        onpointerenter={() => (session.objects.insertionY = session.objects.returnPlace()?.y ?? null)}
        onpointerleave={() => (session.objects.insertionY = null)}
        onfocus={() => (session.objects.insertionY = session.objects.returnPlace()?.y ?? null)}
        onblur={() => (session.objects.insertionY = null)}
        onclick={() => session.objects.dissolve()}>Return to Text</button
      >
    {/if}
    {#if picked > 0}
      <select class="field" aria-label="Line up" title={tooltipFor("wAlignObjects")} value="" onchange={(event) => {
        const value = event.currentTarget.value;
        event.currentTarget.value = "";
        if (value === "h" || value === "v") session.objects.distribute(value === "h");
        else if (value) session.objects.align(value as "left");
      }}>
        <option value="" disabled>Line Up {picked + 1}</option>
        <option value="left">Left Edges</option>
        <option value="center">Centres</option>
        <option value="right">Right Edges</option>
        <option value="top">Tops</option>
        <option value="middle">Middles</option>
        <option value="bottom">Bottoms</option>
        <option value="h">Space Evenly Across</option>
        <option value="v">Space Evenly Down</option>
      </select>
    {/if}
  </div>
  <div class="row numbers">
    <label title="Width">W <input class="field number" value={lengthValue(Number(data.width), unit)} onchange={(event) => setSize("width", event.currentTarget.value)} /></label>
    <label title="Height">H <input class="field number" value={lengthValue(Number(data.height), unit)} onchange={(event) => setSize("height", event.currentTarget.value)} /></label>
    {#if place}
      <label title="From the page’s left edge">X <input class="field number" value={lengthValue(place.x, unit)} onchange={(event) => setPlace("x", event.currentTarget.value)} /></label>
      <label title="From the page’s top edge">Y <input class="field number" value={lengthValue(place.y, unit)} onchange={(event) => setPlace("y", event.currentTarget.value)} /></label>
    {/if}
    <label title={tooltipFor("wImageRotate")}>Angle <input class="field number" value={String(Number(data.rotation ?? 0))} onchange={(event) => onRotation(((Number(event.currentTarget.value) % 360) + 360) % 360 || 0)} />°</label>
    <span class="hint">{hint()} {formatLength(Number(data.width), unit)} wide.</span>
  </div>
</div>

<style>
  .bar {
    position: fixed;
    z-index: 4;
    display: flex;
    flex-direction: column;
    gap: 0.25rem;
    max-width: min(46rem, calc(100vw - 1rem));
    padding: 0.25rem;
  }

  .row {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 0.375rem;
  }

  .numbers label {
    display: flex;
    align-items: center;
    gap: 0.25rem;
    color: var(--label-secondary);
    font: var(--footnote);
  }

  .number {
    width: 3.5rem;
  }

  .pressed {
    background: var(--accent-soft);
    color: var(--accent-label);
  }

  .hint {
    max-width: 18rem;
    color: var(--label-secondary);
    font: var(--footnote);
  }

  .segments {
    display: flex;
    padding: 0.125rem;
    border-radius: calc(var(--radius-small) + 0.125rem);
    background: var(--control);
  }

  .segment {
    padding: 0 0.5rem;
    height: 1.5rem;
    border: 0;
    border-radius: var(--radius-small);
    background: transparent;
    color: var(--label-secondary);
    font: var(--subheadline);
    font-weight: 500;
  }

  .segment.selected {
    background: var(--segment-selected);
    box-shadow: var(--shadow-segment);
    color: var(--label);
  }
</style>

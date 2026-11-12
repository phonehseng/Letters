<script lang="ts">
  // the frame around a selected picture: eight handles to resize it
  // (proportions kept; Shift unlocks, Alt resizes from the centre), a live
  // size readout, snapping to the original size and the text width with a
  // guide, and a small bar: In Line or Free (one click, or Ctrl Alt Shift L and
  // Ctrl Alt Shift F), how text wraps a free picture, Original Size and Fit to
  // Page Width. a free picture moves by dragging it.
  //
  // while dragging only a css transform changes, so nothing on the page
  // moves; the size or place is written once on release, one undo step.
  import { onMount } from "svelte";
  import * as edit from "./editing";
  import { HANDLES, fitWidth, nudgeSize, previewOrigin, resize, snapPosition, type Handle } from "./resize";
  import { tooltipFor } from "../tools";
  import { formatLength, type LengthUnit } from "./units";
  import type { WriterSession } from "./writerSession.svelte";

  interface Props {
    session: WriterSession;
    zoom: number;
    unit: LengthUnit;
  }

  let { session, zoom, unit }: Props = $props();

  type Data = Record<string, unknown>;
  const selected = $derived(edit.selectedImage(session.editor));
  const data = $derived(selected ? (selected.node.attrs.data as Data) : null);
  const free = $derived((data?.placement as { type?: string } | undefined)?.type === "floating");
  const wrap = $derived(free ? String((data?.placement as { wrap?: string }).wrap ?? "square") : "inline");
  let box = $state<DOMRect | null>(null);
  let preview = $state<{ width: number; height: number; dx: number; dy: number; guide: string | null } | null>(null);
  let natural = { width: 0, height: 0 };

  /** the picture's element on screen. */
  function imageElement(): HTMLImageElement | null {
    if (!selected || !session.view) return null;
    const dom = session.view.nodeDOM(selected.pos) as HTMLElement | null;
    return dom?.querySelector("img") ?? null;
  }

  function measure() {
    const image = imageElement();
    box = image && !preview ? image.getBoundingClientRect() : box;
    if (!image) box = null;
    if (image?.naturalWidth) natural = { width: image.naturalWidth, height: image.naturalHeight };
  }

  $effect(() => {
    void session.editor;
    void zoom;
    requestAnimationFrame(measure);
  });

  onMount(() => {
    const update = () => requestAnimationFrame(measure);
    window.addEventListener("scroll", update, true);
    window.addEventListener("resize", update);
    return () => {
      window.removeEventListener("scroll", update, true);
      window.removeEventListener("resize", update);
    };
  });

  const toTwips = (screen: number) => Math.round((screen * 15) / zoom);
  const textWidthScreen = $derived.by(() => {
    const page = session.meta.sections?.[0]?.page;
    return page ? ((page.width - page.margins.left - page.margins.right) / 15) * zoom : 600;
  });

  function startResize(handle: Handle, event: PointerEvent) {
    const image = imageElement();
    if (!image || !box) return;
    event.preventDefault();
    event.stopPropagation();
    (event.currentTarget as HTMLElement).setPointerCapture(event.pointerId);
    const start = { width: box.width, height: box.height };
    const origin = { x: event.clientX, y: event.clientY };
    const snaps = [natural.width * zoom, textWidthScreen].filter((value) => value > 0);
    const move = (moved: PointerEvent) => {
      const result = resize(start, handle, moved.clientX - origin.x, moved.clientY - origin.y, {
        lockAspect: !moved.shiftKey,
        fromCentre: moved.altKey,
        snapWidths: snaps,
      });
      image.style.transformOrigin = previewOrigin(handle, moved.altKey);
      image.style.transform = `scale(${result.width / start.width}, ${result.height / start.height})`;
      preview = { width: result.width, height: result.height, dx: 0, dy: 0, guide: result.snappedTo === null ? null : result.snappedTo === textWidthScreen ? "Text width" : "Original size" };
    };
    const end = () => {
      window.removeEventListener("pointermove", move);
      window.removeEventListener("pointerup", end);
      image.style.transform = "";
      const done = preview;
      preview = null;
      if (done) session.setImageGeometry((current) => ({ ...current, width: toTwips(done.width), height: toTwips(done.height) }));
    };
    window.addEventListener("pointermove", move);
    window.addEventListener("pointerup", end);
  }

  function startMove(event: PointerEvent) {
    const image = imageElement();
    if (!free || !image || !box || !data) return;
    event.preventDefault();
    const origin = { x: event.clientX, y: event.clientY };
    const paragraph = (image.closest("p, h1, h2, h3, h4, h5, h6, td") as HTMLElement | null)?.getBoundingClientRect();
    const startLeft = paragraph ? box.left - paragraph.left : 0;
    const host = image.parentElement as HTMLElement;
    const move = (moved: PointerEvent) => {
      const snapped = snapPosition(startLeft + moved.clientX - origin.x, box!.width, textWidthScreen);
      const dx = snapped.left - startLeft;
      const dy = moved.clientY - origin.y;
      host.style.translate = `${dx}px ${dy}px`;
      preview = { width: box!.width, height: box!.height, dx, dy, guide: snapped.guide ? "Lined up" : null };
    };
    const end = () => {
      window.removeEventListener("pointermove", move);
      window.removeEventListener("pointerup", end);
      host.style.translate = "";
      const done = preview;
      preview = null;
      if (done && (done.dx || done.dy)) {
        session.setImageGeometry((current) => {
          const placement = current.placement as { horizontal: { offset?: number }; vertical: { offset?: number } };
          return {
            ...current,
            placement: {
              ...placement,
              horizontal: { ...placement.horizontal, offset: (placement.horizontal.offset ?? 0) + toTwips(done.dx), align: undefined },
              vertical: { ...placement.vertical, offset: (placement.vertical.offset ?? 0) + toTwips(done.dy) },
            },
          };
        });
      }
    };
    window.addEventListener("pointermove", move);
    window.addEventListener("pointerup", end);
  }

  function originalSize() {
    if (!natural.width) return;
    session.setImageGeometry((current) => ({ ...current, width: natural.width * 15, height: natural.height * 15 }));
  }

  function pageWidth() {
    if (!data) return;
    const page = session.meta.sections?.[0]?.page;
    const limit = page ? page.width - page.margins.left - page.margins.right : 9360;
    const size = fitWidth({ width: Number(data.width), height: Number(data.height) }, limit);
    session.setImageGeometry((current) => ({ ...current, ...(Number(current.width) < limit ? { width: limit, height: Math.round((limit * Number(current.height)) / Number(current.width)) } : size) }));
  }

  // Alt and an arrow resize the selected picture from the keyboard; with
  // Shift too, a free picture moves
  function onKey(event: KeyboardEvent) {
    if (!selected || !data || !event.altKey || !event.key.startsWith("Arrow")) return;
    if (event.ctrlKey || event.metaKey) return;
    event.preventDefault();
    event.stopPropagation();
    const step = unit === "in" ? 90 : 113;
    if (event.shiftKey && free) {
      const dx = event.key === "ArrowLeft" ? -step : event.key === "ArrowRight" ? step : 0;
      const dy = event.key === "ArrowUp" ? -step : event.key === "ArrowDown" ? step : 0;
      session.setImageGeometry((current) => {
        const placement = current.placement as { horizontal: { offset?: number }; vertical: { offset?: number } };
        return { ...current, placement: { ...placement, horizontal: { ...placement.horizontal, offset: (placement.horizontal.offset ?? 0) + dx }, vertical: { ...placement.vertical, offset: (placement.vertical.offset ?? 0) + dy } } };
      });
      return;
    }
    const grow = event.key === "ArrowRight" || event.key === "ArrowUp" ? 1 : -1;
    const next = nudgeSize({ width: Number(data.width), height: Number(data.height) }, grow, step);
    session.setImageGeometry((current) => ({ ...current, ...next }));
  }

  function setWrap(value: string) {
    if (value === "inline") session.setImageFree(false);
    else if (!free) {
      session.setImageFree(true);
      if (value !== "square") session.setImageGeometry((current) => ({ ...current, placement: { ...(current.placement as object), wrap: value } }));
    } else session.setImageGeometry((current) => ({ ...current, placement: { ...(current.placement as object), wrap: value } }));
  }

  const shownWidth = $derived(preview?.width ?? box?.width ?? 0);
  const shownHeight = $derived(preview?.height ?? box?.height ?? 0);
</script>

<svelte:window onkeydown={onKey} />

{#if selected && box}
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div
    class="frame"
    class:movable={free}
    style:left="{box.left + (preview?.dx ?? 0)}px"
    style:top="{box.top + (preview?.dy ?? 0)}px"
    style:width="{shownWidth}px"
    style:height="{shownHeight}px"
    onpointerdown={startMove}
  >
    {#each HANDLES as handle (handle)}
      <span class="handle {handle}" role="presentation" onpointerdown={(event) => startResize(handle, event)}></span>
    {/each}
    {#if preview}
      <span class="readout" role="status">
        {formatLength(toTwips(preview.width), unit)} × {formatLength(toTwips(preview.height), unit)}{preview.guide ? ` · ${preview.guide}` : ""}
      </span>
    {/if}
  </div>
  {#if !preview}
    <div class="glass bar" style:left="{box.left}px" style:top="{Math.max(box.top - 44, 8)}px" role="toolbar" aria-label="Picture">
      <div class="segments" role="radiogroup" aria-label="How the picture sits">
        <button class="segment" class:selected={!free} role="radio" aria-checked={!free} title={tooltipFor("wImageInline")} onclick={() => session.setImageFree(false)}>In Line</button>
        <button class="segment" class:selected={free} role="radio" aria-checked={free} title={tooltipFor("wImageFree")} onclick={() => session.setImageFree(true)}>Free</button>
      </div>
      {#if free}
        <select class="field" aria-label="Text around the picture" value={wrap} onchange={(event) => setWrap(event.currentTarget.value)}>
          <option value="square">Text Around</option>
          <option value="topAndBottom">Above and Below</option>
          <option value="behindText">Behind Text</option>
          <option value="inFrontOfText">In Front of Text</option>
        </select>
      {/if}
      <button class="button" title="Show the picture at the size it was made" onclick={originalSize}>Original Size</button>
      <button class="button" title="Make the picture as wide as the text" onclick={pageWidth}>Fit Width</button>
      <span class="hint">{free ? "Free: drag it anywhere. Word keeps it where you put it." : "In Line: it moves with the text, like a big letter."}</span>
    </div>
  {/if}
{/if}

<style>
  .frame {
    position: fixed;
    z-index: 3;
    box-sizing: border-box;
    border: 1.5px solid var(--accent);
    pointer-events: none;
  }

  .frame.movable {
    pointer-events: auto;
    cursor: move;
  }

  .handle {
    position: absolute;
    width: 0.625rem;
    height: 0.625rem;
    border: 1.5px solid var(--accent);
    border-radius: 50%;
    background: #ffffff;
    pointer-events: auto;
    touch-action: none;
    transform: translate(-50%, -50%);
  }

  .nw { left: 0; top: 0; cursor: nwse-resize; }
  .n { left: 50%; top: 0; cursor: ns-resize; }
  .ne { left: 100%; top: 0; cursor: nesw-resize; }
  .e { left: 100%; top: 50%; cursor: ew-resize; }
  .se { left: 100%; top: 100%; cursor: nwse-resize; }
  .s { left: 50%; top: 100%; cursor: ns-resize; }
  .sw { left: 0; top: 100%; cursor: nesw-resize; }
  .w { left: 0; top: 50%; cursor: ew-resize; }

  .readout {
    position: absolute;
    top: calc(100% + 0.5rem);
    left: 0;
    padding: 0.125rem 0.375rem;
    border-radius: 0.375rem;
    background: var(--glass);
    box-shadow: var(--shadow-glass);
    color: var(--label);
    font: var(--subheadline);
    white-space: nowrap;
  }

  .bar {
    position: fixed;
    z-index: 4;
    display: flex;
    align-items: center;
    gap: 0.375rem;
    padding: 0.25rem;
  }

  .hint {
    max-width: 14rem;
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

<script lang="ts">
  // the frame around the selected picture or text block. eight handles
  // resize it (pictures keep their proportions while Lock Proportions is
  // on; Shift does the opposite, Alt resizes from the centre); the round
  // handle above turns it, in 15 degree steps (Shift frees it); a free
  // object moves by dragging (a text block by its edge) and snaps to the
  // page, the margins and other objects, with guides from the engine's
  // snap rules (snapping.ts). double-click a picture to crop it: the
  // handles then cut its sides instead of scaling it.
  //
  // while dragging only the object's own element changes on screen (a css
  // translate, scale, rotate or its crop window), so nothing else on the
  // page moves or reflows; the result is written once on release, one
  // undo step. ObjectBar.svelte is the small bar of controls above it.
  import { onMount } from "svelte";
  import ObjectBar from "./ObjectBar.svelte";
  import { angleFromPointer, cropDrag, cropOf, cropPlacement, isUncropped, snapAngle, type Crop } from "./crop";
  import { guideOnScreen, keyStep, resizedBox, type Box } from "./frameMaths";
  import { pageAt } from "./objectGeometry";
  import { HANDLES, previewOrigin, type Handle } from "./resize";
  import { snapThreshold, type SnapGuide } from "./snapping";
  import { formatLength, type LengthUnit } from "./units";
  import { rootPoint, selectedObject } from "./writerObjects.svelte";
  import type { WriterSession } from "./writerSession.svelte";

  interface Props {
    session: WriterSession;
    zoom: number;
    unit: LengthUnit;
  }

  let { session, zoom, unit }: Props = $props();

  type Data = Record<string, unknown>;
  const selected = $derived(selectedObject(session.editor));
  const data = $derived(selected ? (selected.node.attrs.data as Data) : null);
  const rotation = $derived(Number(data?.rotation ?? 0));
  const locked = $derived(data?.lockAspectRatio !== false && selected?.kind === "image");

  /** the frame on screen: the object's box before turning, and its centre */
  let frame = $state<Box | null>(null);
  let preview = $state<{ box: Box; angle: number; label: string } | null>(null);
  let guides = $state<{ left: number; top: number; width: number; height: number }[]>([]);
  let cropping = $state(false);
  let natural = $state({ width: 0, height: 0 });

  function element(): HTMLElement | null {
    if (!selected || !session.view) return null;
    return (session.view.nodeDOM(selected.pos) as HTMLElement | null) ?? null;
  }

  /** the box in screen pixels, unturned, from the model's size and the element's centre. */
  function measure() {
    const dom = element();
    if (!dom || !data || preview) {
      if (!dom) frame = null;
      return;
    }
    const rect = dom.getBoundingClientRect();
    const width = (Number(data.width ?? 0) / 15) * zoom;
    const autoHeight = selected?.kind === "textBox" && data.autoHeight;
    const height = autoHeight ? dom.offsetHeight * zoom : (Number(data.height ?? 0) / 15) * zoom;
    const cx = rect.left + rect.width / 2;
    const cy = rect.top + rect.height / 2;
    frame = { left: cx - width / 2, top: cy - height / 2, width, height };
    const image = dom.querySelector("img");
    if (image?.naturalWidth) natural = { width: image.naturalWidth, height: image.naturalHeight };
  }

  $effect(() => {
    void session.editor;
    void zoom;
    requestAnimationFrame(measure);
  });

  $effect(() => {
    // leaving the picture ends cropping
    if (selected?.kind !== "image") cropping = false;
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

  /** the object's top-left in root coordinates, for a screen box. */
  function rootOf(box: Box): { left: number; top: number } {
    const view = session.view!;
    return rootPoint(view.dom, box, zoom);
  }

  // --- dragging, one gesture at a time

  function track(event: PointerEvent, move: (moved: PointerEvent) => void, end: () => void) {
    event.preventDefault();
    event.stopPropagation();
    (event.currentTarget as HTMLElement).setPointerCapture?.(event.pointerId);
    // pointer events can arrive faster than the screen draws: draw once a frame
    let pending: PointerEvent | null = null;
    const onMove = (moved: PointerEvent) => {
      if (!pending) requestAnimationFrame(() => pending && (move(pending), (pending = null)));
      pending = moved;
    };
    const onUp = () => {
      window.removeEventListener("pointermove", onMove);
      window.removeEventListener("pointerup", onUp);
      if (pending) move(pending);
      pending = null;
      end();
    };
    window.addEventListener("pointermove", onMove);
    window.addEventListener("pointerup", onUp);
  }

  function startMove(event: PointerEvent) {
    const dom = element();
    if (!selected || !frame || !dom || !selected.free || cropping || event.button !== 0) return;
    if (event.shiftKey) {
      session.objects.togglePicked(selected.pos);
      return;
    }
    const start = { ...frame };
    const origin = { x: event.clientX, y: event.clientY };
    const layout = session.objects.layout;
    let moved = false;
    track(
      event,
      (pointer) => {
        moved = moved || Math.hypot(pointer.clientX - origin.x, pointer.clientY - origin.y) > 2;
        if (!moved) return;
        const proposed = { ...start, left: start.left + pointer.clientX - origin.x, top: start.top + pointer.clientY - origin.y };
        const root = rootOf(proposed);
        const size = { width: start.width / zoom, height: start.height / zoom };
        const page = layout ? pageAt(root.top, layout) : 0;
        const snapped = session.objects.snapBox(selected.pos, { ...root, ...size }, page, snapThreshold(zoom));
        const dx = (snapped.left - root.left) * zoom + proposed.left - start.left;
        const dy = (snapped.top - root.top) * zoom + proposed.top - start.top;
        dom.style.translate = `${dx / zoom}px ${dy / zoom}px`;
        preview = { box: { ...start, left: start.left + dx, top: start.top + dy }, angle: rotation, label: positionLabel(snapped.guides) };
        guides = screenGuides(snapped.guides, page);
      },
      () => {
        const done = preview;
        dom.style.translate = "";
        preview = null;
        guides = [];
        if (!moved || !done) {
          if (selected.kind === "textBox") session.selectNode(selected.pos);
          return;
        }
        const root = rootOf(done.box);
        session.objects.moveTo(selected.pos, root.left, root.top);
      },
    );
  }

  function positionLabel(found: SnapGuide[]): string {
    if (!found.length) return "";
    const kinds = [...new Set(found.map((guide) => guide.kind))];
    return `Lined up with the ${kinds.join(" and the ")}`;
  }

  function screenGuides(found: SnapGuide[], page: number) {
    const layout = session.objects.layout;
    const view = session.view;
    if (!layout || !view) return [];
    const root = view.dom.getBoundingClientRect();
    return found.map((guide) =>
      guideOnScreen(guide, { edgeTop: layout.pageEdges[page] ?? 0, widthTwips: layout.pageWidth, heightTwips: layout.pageHeight, marginLeftTwips: layout.margins.left }, root, zoom),
    );
  }

  function startResize(handle: Handle, event: PointerEvent) {
    const dom = element();
    if (!selected || !frame || !dom || !data) return;
    if (cropping) return startCrop(handle, event);
    const start = { ...frame };
    const origin = { x: event.clientX, y: event.clientY };
    const isBox = selected.kind === "textBox";
    const snaps = isBox ? [] : [natural.width * zoom, textWidthScreen].filter((value) => value > 0);
    track(
      event,
      (pointer) => {
        const box = resizedBox(start, handle, pointer.clientX - origin.x, pointer.clientY - origin.y, {
          lockAspect: locked !== pointer.shiftKey,
          fromCentre: pointer.altKey,
          snapWidths: snaps,
        });
        if (isBox) {
          // a text block's text reflows inside it as it is resized
          dom.style.width = `${box.width / zoom}px`;
          dom.style[data.autoHeight ? "minHeight" : "height"] = `${box.height / zoom}px`;
          dom.style.translate = `${(box.left - start.left) / zoom}px ${(box.top - start.top) / zoom}px`;
        } else {
          dom.style.transformOrigin = previewOrigin(handle, pointer.altKey);
          dom.style.scale = `${box.width / start.width} ${box.height / start.height}`;
        }
        const snapped = box.snappedTo === null ? "" : box.snappedTo === textWidthScreen ? " · Text width" : " · Original size";
        preview = { box, angle: rotation, label: `${formatLength(toTwips(box.width), unit)} × ${formatLength(toTwips(box.height), unit)}${snapped}` };
      },
      () => {
        const done = preview;
        Object.assign(dom.style, { scale: "", translate: "", transformOrigin: "", width: "", height: "", minHeight: "" });
        preview = null;
        if (!done) return;
        commitBox(done.box, start);
      },
    );
  }

  /** writes a new size (and, for a free object whose top or left moved, its new place). */
  function commitBox(box: Box, start: Box) {
    if (!selected) return;
    const pos = selected.pos;
    const width = toTwips(box.width);
    const height = toTwips(box.height);
    const isBox = selected.kind === "textBox";
    session.objects.setData(pos, (current) => ({ ...current, width, height, ...(isBox && Math.abs(box.height - start.height) > 1 ? { autoHeight: false } : {}) }));
    if (selected.free && (Math.abs(box.left - start.left) > 0.5 || Math.abs(box.top - start.top) > 0.5)) {
      const root = rootOf(box);
      session.objects.moveTo(pos, root.left, root.top);
    }
  }

  function startRotate(event: PointerEvent) {
    const dom = element();
    if (!selected || !frame || !dom) return;
    const start = { ...frame };
    const centre = { x: start.left + start.width / 2, y: start.top + start.height / 2 };
    track(
      event,
      (pointer) => {
        const angle = snapAngle(angleFromPointer(centre, { x: pointer.clientX, y: pointer.clientY }), pointer.shiftKey);
        dom.style.transform = `rotate(${angle}deg)`;
        preview = { box: start, angle, label: `${Math.round(angle * 10) / 10}°` };
      },
      () => {
        const done = preview;
        dom.style.transform = "";
        preview = null;
        if (done) setRotation(done.angle);
      },
    );
  }

  function setRotation(angle: number) {
    if (!selected) return;
    session.objects.setData(selected.pos, (current) => {
      const next = { ...current };
      if (angle) next.rotation = angle;
      else delete next.rotation;
      return next;
    });
  }

  // --- cropping

  function startCrop(handle: Handle, event: PointerEvent) {
    const dom = element();
    if (!selected || !frame || !dom || !data) return;
    const start = { ...frame };
    const startCropValue = cropOf(data);
    const sizePx = { width: Number(data.width) / 15, height: Number(data.height) / 15 };
    const origin = { x: event.clientX, y: event.clientY };
    let result: ReturnType<typeof cropDrag> | null = null;
    track(
      event,
      (pointer) => {
        result = cropDrag(sizePx, startCropValue, handle, (pointer.clientX - origin.x) / zoom, (pointer.clientY - origin.y) / zoom);
        drawCrop(dom, result.width, result.height, result.crop);
        dom.style.translate = `${result.shiftX}px ${result.shiftY}px`;
        const box = { left: start.left + result.shiftX * zoom, top: start.top + result.shiftY * zoom, width: result.width * zoom, height: result.height * zoom };
        preview = { box, angle: rotation, label: `${formatLength(toTwips(box.width), unit)} × ${formatLength(toTwips(box.height), unit)} shown` };
      },
      () => {
        dom.style.translate = "";
        const done = preview;
        preview = null;
        if (!result || !done) return;
        commitCrop(result.crop, done.box, start);
      },
    );
  }

  /** the picture's crop window, redrawn while dragging (the node view draws it after). */
  function drawCrop(dom: HTMLElement, width: number, height: number, crop: Crop) {
    let window = dom.querySelector<HTMLElement>(".w-crop");
    const image = dom.querySelector<HTMLImageElement>("img");
    if (!image) return;
    if (!window) {
      window = document.createElement("span");
      window.className = "w-crop";
      image.replaceWith(window);
      window.append(image);
    }
    window.style.width = `${width}px`;
    window.style.height = `${height}px`;
    const whole = cropPlacement({ width, height }, crop);
    Object.assign(image.style, { left: `${whole.left}px`, top: `${whole.top}px`, width: `${whole.width}px`, height: `${whole.height}px` });
  }

  function commitCrop(crop: Crop, box: Box, start: Box) {
    if (!selected) return;
    const width = toTwips(box.width);
    const height = toTwips(box.height);
    session.objects.setData(selected.pos, (current) => {
      const next: Data = { ...current, width, height };
      if (isUncropped(crop)) delete next.crop;
      else next.crop = crop;
      return next;
    });
    if (selected.free && (Math.abs(box.left - start.left) > 0.5 || Math.abs(box.top - start.top) > 0.5)) {
      const root = rootOf(box);
      session.objects.moveTo(selected.pos, root.left, root.top);
    }
  }

  export function toggleCrop() {
    if (selected?.kind !== "image") {
      session.say("Click a picture first.");
      return;
    }
    cropping = !cropping;
    session.announce(cropping ? "Cropping: drag a handle to cut that side. Press Return when done." : "Done cropping.");
  }

  // --- the keyboard: Alt and an arrow resizes, Alt Shift and an arrow moves

  function onKey(event: KeyboardEvent) {
    if (!selected || !data || !frame) return;
    if (cropping && (event.key === "Escape" || event.key === "Enter")) {
      event.preventDefault();
      cropping = false;
      session.announce("Done cropping.");
      return;
    }
    if (!event.altKey || !event.key.startsWith("Arrow") || event.ctrlKey || event.metaKey) return;
    event.preventDefault();
    event.stopPropagation();
    const step = (keyStep(unit) / 15) * zoom;
    const dx = event.key === "ArrowLeft" ? -step : event.key === "ArrowRight" ? step : 0;
    const dy = event.key === "ArrowUp" ? -step : event.key === "ArrowDown" ? step : 0;
    if (event.shiftKey) {
      if (!selected.free) return;
      const root = rootOf({ ...frame, left: frame.left + dx, top: frame.top + dy });
      session.objects.moveTo(selected.pos, root.left, root.top);
      session.announce(`Moved ${event.key.replace("Arrow", "").toLowerCase()}.`);
      return;
    }
    // right and down grow, left and up shrink
    const grow = event.key === "ArrowRight" || event.key === "ArrowDown" ? 1 : -1;
    const box = resizedBox(frame, "se", grow * step, grow * step, { lockAspect: locked, fromCentre: false });
    commitBox(box, frame);
    session.announce(`${formatLength(toTwips(box.width), unit)} by ${formatLength(toTwips(box.height), unit)}.`);
  }

  function onDoubleClick(event: MouseEvent) {
    if (selected?.kind === "image") {
      event.preventDefault();
      toggleCrop();
    }
  }

  const shown = $derived(preview?.box ?? frame);
  const shownAngle = $derived(preview?.angle ?? rotation);
  const insertion = $derived.by(() => {
    const y = session.objects.insertionY;
    const view = session.view;
    if (y === null || !view) return null;
    const root = view.dom.getBoundingClientRect();
    return { left: root.left, top: root.top + y * zoom, width: textWidthScreen };
  });
</script>

<svelte:window onkeydown={onKey} />

{#if selected && shown}
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div
    class="frame"
    class:movable={selected.free && selected.kind === "image" && !cropping}
    class:box={selected.kind === "textBox"}
    class:cropping
    style:left="{shown.left}px"
    style:top="{shown.top}px"
    style:width="{shown.width}px"
    style:height="{shown.height}px"
    style:rotate="{shownAngle}deg"
    onpointerdown={startMove}
    ondblclick={onDoubleClick}
  >
    {#if selected.kind === "textBox"}
      <!-- a text block is moved by its edge, so a click inside places the cursor -->
      {#each ["top", "right", "bottom", "left"] as side (side)}
        <span class="edge {side}" role="presentation" onpointerdown={startMove}></span>
      {/each}
    {/if}
    {#each HANDLES as handle (handle)}
      <span class="handle {handle}" role="presentation" onpointerdown={(event) => startResize(handle, event)}></span>
    {/each}
    {#if !cropping}
      <span class="rotate" role="presentation" title="Drag to turn. Steps of 15°; hold Shift for any angle." onpointerdown={startRotate}></span>
    {/if}
    {#if preview}
      <span class="readout" role="status">{preview.label}</span>
    {/if}
  </div>
  {#if !preview}
    <ObjectBar {session} {selected} {unit} {cropping} {natural} box={shown} onCrop={toggleCrop} onRotation={setRotation} />
  {/if}
{/if}

{#each guides as guide, index (index)}
  <div class="guide" class:vertical={guide.width === 0} style:left="{guide.left}px" style:top="{guide.top}px" style:width="{guide.width}px" style:height="{guide.height}px"></div>
{/each}

{#if insertion}
  <div class="insertion" style:left="{insertion.left}px" style:top="{insertion.top}px" style:width="{insertion.width}px" aria-hidden="true"></div>
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

  .frame.cropping {
    border-style: dashed;
  }

  .edge {
    position: absolute;
    pointer-events: auto;
    cursor: move;
  }

  .edge.top,
  .edge.bottom {
    left: 0;
    right: 0;
    height: 0.5rem;
  }

  .edge.top {
    top: -0.3rem;
  }

  .edge.bottom {
    bottom: -0.3rem;
  }

  .edge.left,
  .edge.right {
    top: 0;
    bottom: 0;
    width: 0.5rem;
  }

  .edge.left {
    left: -0.3rem;
  }

  .edge.right {
    right: -0.3rem;
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

  /* crop handles are bars, so they read as "cut here" rather than "pull" */
  .cropping .handle {
    width: 0.75rem;
    height: 0.75rem;
    border-radius: 1px;
    background: var(--accent);
  }

  .rotate {
    position: absolute;
    left: 50%;
    top: -1.5rem;
    width: 0.75rem;
    height: 0.75rem;
    border: 1.5px solid var(--accent);
    border-radius: 50%;
    background: #ffffff;
    pointer-events: auto;
    cursor: grab;
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
    rotate: 0deg;
  }

  .guide {
    position: fixed;
    z-index: 3;
    border-top: 1px solid #ff3b82;
    pointer-events: none;
  }

  .guide.vertical {
    border-top: 0;
    border-left: 1px solid #ff3b82;
  }

  .insertion {
    position: fixed;
    z-index: 3;
    height: 0;
    border-top: 2px solid var(--accent);
    pointer-events: none;
  }
</style>

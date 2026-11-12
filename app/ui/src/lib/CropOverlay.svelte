<script lang="ts">
  // the crop rectangle over the page being read, while the Crop tool is
  // open. the part that will be hidden is shaded; the rectangle moves when
  // dragged and resizes from its eight handles. from the keyboard, the
  // arrows move it and shift with the arrows changes its size.
  //
  // the geometry (and the rule that nothing leaves the page) is in cropRect.ts.
  import { dragCrop, HANDLES, keepOnPage, type CropBox, type CropHandle } from "./cropRect";
  import type { PageSize, PlacedPage } from "./layout";

  interface Props {
    page: PlacedPage;
    /** the page's size in points, as shown */
    size: PageSize;
    /** the page's left edge in the scroll area, css pixels */
    left: number;
    /** css pixels per point */
    zoom: number;
    box: CropBox;
    onChange: (box: CropBox) => void;
  }

  let { page, size, left, zoom, box, onChange }: Props = $props();

  let drag: { handle: CropHandle; startX: number; startY: number; startBox: CropBox } | null = null;

  function start(event: PointerEvent, handle: CropHandle) {
    if (event.button !== 0) return;
    event.preventDefault();
    event.stopPropagation();
    (event.currentTarget as HTMLElement).setPointerCapture(event.pointerId);
    drag = { handle, startX: event.clientX, startY: event.clientY, startBox: box };
  }

  function move(event: PointerEvent) {
    if (!drag) return;
    const dx = (event.clientX - drag.startX) / zoom;
    const dy = (event.clientY - drag.startY) / zoom;
    onChange(dragCrop(drag.startBox, drag.handle, dx, dy, size));
  }

  function end() {
    drag = null;
  }

  function onKeyDown(event: KeyboardEvent) {
    const steps: Record<string, [number, number]> = {
      ArrowLeft: [-1, 0],
      ArrowRight: [1, 0],
      ArrowUp: [0, -1],
      ArrowDown: [0, 1],
    };
    const step = steps[event.key];
    if (!step) return;
    event.preventDefault();
    // alt moves ten points at a time, for big changes without holding the key
    const amount = event.altKey ? 10 : 1;
    const [dx, dy] = [step[0] * amount, step[1] * amount];
    const next = event.shiftKey
      ? { ...box, width: box.width + dx, height: box.height + dy }
      : { ...box, left: box.left + dx, top: box.top + dy };
    onChange(keepOnPage(next, size));
  }
</script>

<div
  class="crop"
  style:left="{left}px"
  style:top="{page.top}px"
  style:width="{page.width}px"
  style:height="{page.height}px"
>
  <!-- the rectangle is a custom control: "application" tells screen readers
       to pass the arrow keys through to it, and its label says how to use them -->
  <!-- svelte-ignore a11y_no_noninteractive_tabindex, a11y_no_noninteractive_element_interactions -->
  <div
    class="box"
    role="application"
    tabindex="0"
    aria-label="Crop rectangle. Drag it or use the arrow keys to move it; Shift with the arrows changes its size."
    style:left="{box.left * zoom}px"
    style:top="{box.top * zoom}px"
    style:width="{box.width * zoom}px"
    style:height="{box.height * zoom}px"
    onpointerdown={(event) => start(event, "move")}
    onpointermove={move}
    onpointerup={end}
    onpointercancel={end}
    onkeydown={onKeyDown}
  >
    {#each HANDLES as handle (handle)}
      <span
        class="handle {handle}"
        aria-hidden="true"
        onpointerdown={(event) => start(event, handle)}
        onpointermove={move}
        onpointerup={end}
        onpointercancel={end}
      ></span>
    {/each}
  </div>
</div>

<style>
  .crop {
    position: absolute;
    overflow: hidden;
    pointer-events: none;
  }

  /* everything outside the rectangle is shaded: that is what will be hidden */
  .box {
    position: absolute;
    border: 1px solid var(--accent);
    box-shadow: 0 0 0 100vmax rgb(0 0 0 / 0.38);
    cursor: move;
    pointer-events: auto;
    touch-action: none;
  }

  .box:focus-visible {
    outline: 3px solid var(--accent-soft);
    outline-offset: 0;
  }

  .handle {
    position: absolute;
    width: 0.75rem;
    height: 0.75rem;
    border: 1px solid var(--accent);
    border-radius: 2px;
    background: #ffffff;
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
</style>

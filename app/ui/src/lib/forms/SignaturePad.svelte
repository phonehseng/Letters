<script lang="ts">
  // a pad to sign on with the mouse, a pen or a finger. strokes are
  // smoothed (see smoothPath in signaturePicture.ts) and drawn in dark
  // ink; Clear starts again; Keep Signature makes a png of what was drawn,
  // cut to the ink, with a see-through background.
  //
  // drawing needs a pointer; from the keyboard, Type a signature instead.
  import { strokesBounds, smoothPath } from "./signaturePicture";
  import type { Point } from "../pageTools/pagePoints";
  import { canvasPng } from "./canvasPng";

  interface Props {
    onKeep: (png: Uint8Array) => Promise<unknown>;
  }

  let { onKeep }: Props = $props();

  /** the pen, in css pixels on the pad */
  const PEN_WIDTH = 2.6;
  const INK = "#1b1f3b";
  /** the png is drawn at this many pixels per css pixel, so it prints sharply */
  const DETAIL = 3;

  let canvas = $state<HTMLCanvasElement>();
  let strokes = $state<Point[][]>([]);
  let current: Point[] | null = null;
  let busy = $state(false);

  function pointOf(event: PointerEvent): Point {
    const box = canvas!.getBoundingClientRect();
    return { x: event.clientX - box.left, y: event.clientY - box.top };
  }

  function redraw() {
    const context = canvas?.getContext("2d");
    if (!canvas || !context) return;
    const box = canvas.getBoundingClientRect();
    canvas.width = Math.round(box.width * DETAIL);
    canvas.height = Math.round(box.height * DETAIL);
    context.setTransform(DETAIL, 0, 0, DETAIL, 0, 0);
    context.lineWidth = PEN_WIDTH;
    context.lineCap = "round";
    context.lineJoin = "round";
    context.strokeStyle = INK;
    for (const stroke of current ? [...strokes, current] : strokes) context.stroke(new Path2D(smoothPath(stroke)));
  }

  function down(event: PointerEvent) {
    if (event.button !== 0 || !canvas) return;
    event.preventDefault();
    canvas.setPointerCapture(event.pointerId);
    current = [pointOf(event)];
    redraw();
  }

  function move(event: PointerEvent) {
    if (!current) return;
    // every point the system saw, so a quick flourish keeps its shape
    const events = typeof event.getCoalescedEvents === "function" ? event.getCoalescedEvents() : [];
    for (const each of events.length > 0 ? events : [event]) current.push(pointOf(each));
    redraw();
  }

  function up() {
    if (!current) return;
    strokes = [...strokes, current];
    current = null;
    redraw();
  }

  function clear() {
    strokes = [];
    current = null;
    redraw();
  }

  async function keep() {
    const bounds = strokesBounds(strokes, PEN_WIDTH + 2);
    if (!canvas || !bounds) return;
    busy = true;
    try {
      const crop = { x: Math.max(0, bounds.x * DETAIL), y: Math.max(0, bounds.y * DETAIL), width: bounds.width * DETAIL, height: bounds.height * DETAIL };
      if (await onKeep(await canvasPng(canvas, crop))) clear();
    } finally {
      busy = false;
    }
  }
</script>

<div class="pad-area">
  <canvas
    bind:this={canvas}
    class="pad"
    aria-label="Signature pad. Draw your signature here with the mouse, a pen or a finger."
    onpointerdown={down}
    onpointermove={move}
    onpointerup={up}
    onpointercancel={up}
  ></canvas>
  <span class="line" aria-hidden="true"></span>
  {#if strokes.length === 0}<span class="placeholder" aria-hidden="true">Sign here</span>{/if}
</div>
<div class="buttons">
  <button class="button" disabled={strokes.length === 0} onclick={clear}>Clear</button>
  <button class="button" disabled={strokes.length === 0 || busy} onclick={keep}>Keep Signature</button>
</div>
<p class="hint">Drawing needs a mouse, pen or finger. From the keyboard, choose Type instead.</p>

<style>
  .pad-area {
    position: relative;
  }

  /* white like paper in both light and dark mode, since the ink is dark */
  .pad {
    display: block;
    width: 100%;
    height: 7.5rem;
    border-radius: var(--radius-small);
    background: #ffffff;
    box-shadow: inset 0 0 0 0.5px var(--separator);
    touch-action: none;
    cursor: crosshair;
  }

  .line {
    position: absolute;
    right: 1rem;
    bottom: 1.75rem;
    left: 1rem;
    border-bottom: 1px solid rgb(0 0 0 / 0.2);
    pointer-events: none;
  }

  .placeholder {
    position: absolute;
    bottom: 0.75rem;
    left: 1rem;
    color: rgb(0 0 0 / 0.35);
    font: var(--subheadline);
    pointer-events: none;
  }

  .buttons {
    display: flex;
    justify-content: flex-end;
    gap: 0.5rem;
  }
</style>

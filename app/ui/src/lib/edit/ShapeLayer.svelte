<script lang="ts">
  // the vector shapes of one page while Edit Text and Images is on: each
  // shape is a target (a hairline gets a target wide enough to click), a
  // click selects it or the logo it belongs to, and the selection gets the
  // same frame as pictures (drag to move, handles to resize) plus a small
  // bar. one rectangle or line also shows a handle on each corner or end,
  // to reshape it without stretching its line width.
  //
  // the overlay draws it twice: `targets` inside the page's own area (so
  // the targets scroll and scale with it, under the text and pictures),
  // and `selection` beside the picture frame.
  import ObjectFrame from "../pageTools/ObjectFrame.svelte";
  import { HANDLES } from "../pageTools/objectFrame";
  import type { PageFrame, Point } from "../pageTools/pagePoints";
  import type { PathObject } from "./editApi";
  import type { EditSession } from "./editSession.svelte";
  import { movedPoints, targetRect } from "./shapeGeometry";

  interface Props {
    session: EditSession;
    frame: PageFrame;
    part: "targets" | "selection";
    area?: HTMLDivElement;
    onFocusDocument?: () => void;
  }

  let { session, frame, part, area, onFocusDocument = () => {} }: Props = $props();

  // points a key press nudges a corner by; shift for ten, as frames do
  const NUDGE = 1;
  const BIG_NUDGE = 10;
  // how long the arrows rest before a reshape is made, as the frame waits
  const KEY_COMMIT_MS = 450;
  // a shape covering more of the page than this is its background or a
  // frame: it gets no target, so a click on empty page still lets go
  const BACKGROUND_SHARE = 0.5;

  const shapes = $derived(session.shapes);
  const pageArea = $derived(frame.size.width * frame.size.height);
  const paths = $derived(
    session.tool === "edit" && part === "targets"
      ? (session.objects.get(frame.index)?.paths ?? []).filter((path) => path.rect.width * path.rect.height < pageArea * BACKGROUND_SHARE)
      : [],
  );
  const selectedHere = $derived(shapes.selected?.page === frame.index ? shapes.rect : null);
  const pointShape = $derived(shapes.selected?.page === frame.index ? shapes.pointShape : null);
  const points = $derived(shapes.pointPreview ?? pointShape?.points ?? []);
  let keyTimer: ReturnType<typeof setTimeout> | undefined;

  function px(value: number) {
    return `${value * frame.scale}px`;
  }

  function pointOf(event: PointerEvent): Point | null {
    const box = area?.getBoundingClientRect();
    if (!box) return null;
    return { x: (event.clientX - box.left) / frame.scale, y: (event.clientY - box.top) / frame.scale };
  }

  function label(path: PathObject): string {
    const kind = path.shape === "free" ? "drawing" : path.shape;
    return `A ${kind} on page ${frame.index + 1}. Press Enter to select it.`;
  }

  function dragPoint(event: PointerEvent, index: number) {
    if (!pointShape) return;
    event.stopPropagation();
    const shape = pointShape;
    const handle = event.currentTarget as HTMLElement;
    handle.setPointerCapture(event.pointerId);
    const move = (moved: PointerEvent) => {
      const at = pointOf(moved);
      if (at) shapes.pointPreview = movedPoints(shape.shape, shape.points, index, at);
    };
    const up = () => {
      handle.removeEventListener("pointermove", move);
      handle.removeEventListener("pointerup", up);
      if (shapes.pointPreview) shapes.setPoints(shapes.pointPreview);
    };
    handle.addEventListener("pointermove", move);
    handle.addEventListener("pointerup", up);
  }

  function nudgePoint(event: KeyboardEvent, index: number) {
    if (!pointShape) return;
    const step = event.shiftKey ? BIG_NUDGE : NUDGE;
    const moves: Record<string, [number, number]> = { ArrowLeft: [-step, 0], ArrowRight: [step, 0], ArrowUp: [0, -step], ArrowDown: [0, step] };
    const move = moves[event.key];
    if (!move) return;
    event.preventDefault();
    const from = points[index];
    shapes.pointPreview = movedPoints(pointShape.shape, points, index, { x: from.x + move[0], y: from.y + move[1] });
    clearTimeout(keyTimer);
    keyTimer = setTimeout(() => shapes.pointPreview && shapes.setPoints(shapes.pointPreview), KEY_COMMIT_MS);
  }
</script>

{#each paths as path (path.id)}
  {@const target = targetRect(path.rect)}
  <button
    class="shape-target"
    data-page-ui
    data-path-id={path.id}
    aria-label={label(path)}
    style:left={px(target.x)}
    style:top={px(target.y)}
    style:width={px(target.width)}
    style:height={px(target.height)}
    onpointerdown={(event) => {
      event.stopPropagation();
      shapes.select(frame.index, path.id);
    }}
    onclick={(event) => event.detail === 0 && shapes.select(frame.index, path.id)}
  ></button>
{/each}

{#if part === "selection" && selectedHere && shapes.selected}
  {#key shapes.selected.ids.join(" ")}
    <ObjectFrame
      {frame}
      rect={shapes.preview ?? selectedHere}
      label={`Shape on page ${frame.index + 1}. The arrow keys move it, with Shift for 10 points; Alt with the arrows resizes it. Delete deletes it. Escape lets go.`}
      movable={true}
      handles={HANDLES}
      minSize={1}
      focusOnShow={true}
      onPreview={(rect) => (shapes.preview = rect)}
      onCommit={(rect) => shapes.place(rect)}
      onDelete={() => shapes.delete()}
      onEscape={() => {
        shapes.deselect();
        onFocusDocument();
      }}
    />
  {/key}
  {#each points as point, index (index)}
    <button
      class="point"
      data-page-ui
      data-point={index}
      aria-label={`Point ${index + 1} of the ${pointShape?.shape}. Drag it, or use the arrow keys.`}
      style:left="{frame.left + point.x * frame.scale}px"
      style:top="{frame.top + point.y * frame.scale}px"
      onpointerdown={(event) => dragPoint(event, index)}
      onkeydown={(event) => nudgePoint(event, index)}
    ></button>
  {/each}
  <div
    class="shape-bar glass"
    role="toolbar"
    aria-label="Shape"
    data-page-ui
    style:left="{frame.left + selectedHere.x * frame.scale}px"
    style:top="{frame.top + Math.max(0, selectedHere.y * frame.scale - 40)}px"
  >
    <button class="button" title="Turn the shape a quarter turn to the left" onclick={() => shapes.rotate(-90)}>Turn Left</button>
    <button class="button" title="Turn the shape a quarter turn to the right" onclick={() => shapes.rotate(90)}>Turn Right</button>
    <button class="button" title="Make a copy of the shape, a little down and to the right" onclick={() => shapes.duplicate()}>Duplicate</button>
    <button class="button" title="Remove the shape from the page" onclick={() => shapes.delete()}>Delete</button>
  </div>
{/if}

<style>
  .shape-target {
    position: absolute;
    margin: 0;
    padding: 0;
    border: 0;
    background: transparent;
    box-shadow: none;
    pointer-events: auto;
    cursor: pointer;
  }

  .shape-target:hover,
  .shape-target:focus-visible {
    outline: 1px dashed rgb(0 136 255 / 0.7);
  }

  .point {
    position: absolute;
    z-index: 4;
    width: 0.625rem;
    height: 0.625rem;
    margin: -0.3125rem 0 0 -0.3125rem;
    padding: 0;
    border: 1.5px solid var(--accent);
    border-radius: 50%;
    background: white;
    cursor: move;
  }

  .shape-bar {
    position: absolute;
    z-index: 3;
    display: flex;
    gap: 0.25rem;
    padding: var(--group-padding);
  }
</style>

<script lang="ts">
  // the frame around a selected object: a thin accent outline, eight
  // square handles for resizing (none when several objects are selected),
  // and a round handle above it for turning, as in the writer's object
  // frame (lib/writer/ObjectFrame.svelte) and in keynote. while a handle
  // is dragged the frame shows the new box and a small label says its size.
  import { HANDLES, type Handle } from "../writer/resize";
  import type { Box } from "./geometry";

  interface Props {
    box: Box;
    rotation: number;
    scale: number;
    /** false for several objects, lines and turned groups: only moving applies */
    resizable: boolean;
    turnable: boolean;
    sizeLabel?: string | null;
    onHandle: (handle: Handle, event: PointerEvent) => void;
    onTurn: (event: PointerEvent) => void;
  }

  let { box, rotation, scale, resizable, turnable, sizeLabel = null, onHandle, onTurn }: Props = $props();

  /** where each handle sits, as shares of the box */
  const SPOTS: Record<Handle, [number, number]> = {
    nw: [0, 0],
    n: [0.5, 0],
    ne: [1, 0],
    e: [1, 0.5],
    se: [1, 1],
    s: [0.5, 1],
    sw: [0, 1],
    w: [0, 0.5],
  };

  /** the cursor of each handle, turned with the object to the nearest of the four */
  function cursorFor(handle: Handle): string {
    const order: Handle[] = ["n", "ne", "e", "se", "s", "sw", "w", "nw"];
    const steps = Math.round((((rotation % 360) + 360) % 360) / 45);
    const turned = order[(order.indexOf(handle) + steps) % 8];
    const names: Record<Handle, string> = { n: "ns", s: "ns", e: "ew", w: "ew", ne: "nesw", sw: "nesw", nw: "nwse", se: "nwse" };
    return `${names[turned]}-resize`;
  }
</script>

<div
  class="frame"
  style:left="{box.x * scale}px"
  style:top="{box.y * scale}px"
  style:width="{box.width * scale}px"
  style:height="{box.height * scale}px"
  style:transform="rotate({rotation}deg)"
>
  {#if turnable}
    <span class="stalk" aria-hidden="true"></span>
    <button
      class="turn"
      tabindex="-1"
      aria-label="Turn. Shift turns freely."
      title="Drag to turn in 15° steps (Shift for any angle)"
      onpointerdown={(event) => {
        event.stopPropagation();
        onTurn(event);
      }}
    ></button>
  {/if}
  {#if resizable}
    {#each HANDLES as handle (handle)}
      <button
        class="handle"
        tabindex="-1"
        aria-label="Resize from the {handle} handle"
        style:left="{SPOTS[handle][0] * 100}%"
        style:top="{SPOTS[handle][1] * 100}%"
        style:cursor={cursorFor(handle)}
        onpointerdown={(event) => {
          event.stopPropagation();
          onHandle(handle, event);
        }}
      ></button>
    {/each}
  {/if}
  {#if sizeLabel}
    <span class="size" style:transform="rotate({-rotation}deg)">{sizeLabel}</span>
  {/if}
</div>

<style>
  .frame {
    position: absolute;
    z-index: 2;
    box-sizing: border-box;
    outline: 1px solid var(--accent);
    pointer-events: none;
  }

  .handle {
    position: absolute;
    width: 0.5rem;
    height: 0.5rem;
    margin: -0.25rem 0 0 -0.25rem;
    padding: 0;
    border: 1px solid var(--accent);
    border-radius: 1px;
    background: #ffffff;
    pointer-events: auto;
  }

  .stalk {
    position: absolute;
    left: 50%;
    top: -1.25rem;
    height: 1.25rem;
    border-left: 1px solid var(--accent);
  }

  .turn {
    position: absolute;
    left: 50%;
    top: -1.25rem;
    width: 0.625rem;
    height: 0.625rem;
    margin: -0.3125rem 0 0 -0.3125rem;
    padding: 0;
    border: 1px solid var(--accent);
    border-radius: 50%;
    background: #ffffff;
    cursor: grab;
    pointer-events: auto;
  }

  .size {
    position: absolute;
    left: 50%;
    top: calc(100% + 0.5rem);
    translate: -50% 0;
    padding: 0.125rem 0.375rem;
    border-radius: var(--radius-small);
    background: var(--glass);
    box-shadow: var(--shadow-glass);
    font: var(--footnote);
    font-variant-numeric: tabular-nums;
    white-space: nowrap;
  }
</style>

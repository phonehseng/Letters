<script lang="ts">
  // the frame around a selected object on a page: an annotation, or the
  // crop rectangle. drag inside it to move it, drag one of its handles to
  // resize it. from the keyboard the arrows move it (shift for 10 points),
  // alt with the arrows resizes it, Delete deletes it and Escape lets go.
  //
  // while it is dragged or nudged, `onPreview` gets each new place so the
  // page can show it at once; `onCommit` gets the final place once, when
  // the drag ends or the arrow keys have rested a moment, so holding an
  // arrow key makes one change and one undo step, not dozens.
  //
  // the geometry and its rules are in objectFrame.ts.
  import { tick } from "svelte";
  import { cursorFor, dragFrame, frameAfterKey, HANDLE_SPOTS, sameRect, type Handle, type ResizeHandle } from "./objectFrame";
  import type { PageFrame, Rect } from "./pagePoints";

  interface Props {
    frame: PageFrame;
    rect: Rect;
    /** what a screen reader says about it, with how to use it */
    label: string;
    movable: boolean;
    handles: ResizeHandle[];
    keepAspect?: boolean;
    minSize?: number;
    /** shade everything outside it, as the crop rectangle does */
    shade?: boolean;
    /** how long the arrow keys must rest before their change is made, in ms */
    keyCommitDelay?: number;
    /** a different keyboard, such as the crop rectangle's (shift resizes there) */
    keyRect?: (event: KeyboardEvent, rect: Rect) => Rect | null;
    onPreview?: (rect: Rect) => void;
    onCommit: (rect: Rect) => void;
    onDelete?: () => void;
    onEscape?: () => void;
    onOpen?: () => void;
    /** starts with the focus, so the keyboard works at once */
    focusOnShow?: boolean;
    /** a class naming what the frame is for, such as "crop" */
    name?: string;
  }

  let {
    frame,
    rect,
    label,
    movable,
    handles,
    keepAspect = false,
    minSize = 4,
    shade = false,
    keyCommitDelay = 450,
    keyRect,
    onPreview,
    onCommit,
    onDelete,
    onEscape,
    onOpen,
    focusOnShow = false,
    name = "",
  }: Props = $props();

  let box = $state<HTMLDivElement>();
  let drag: { handle: Handle; startX: number; startY: number; startRect: Rect; latest: Rect } | null = null;
  // the place the arrow keys have reached, before it is committed, and
  // where the burst of presses started (the preview moves `rect` itself,
  // so it can't be what the result is compared with)
  let keyed: Rect | null = null;
  let keyedFrom: Rect | null = null;
  let keyTimer: ReturnType<typeof setTimeout> | undefined;

  $effect(() => {
    if (focusOnShow) tick().then(() => box?.focus({ preventScroll: true }));
  });

  // a change still waiting is made when the frame goes away
  $effect(() => () => commitKeys());

  function rules() {
    return { page: frame.size, minSize, keepAspect };
  }

  function start(event: PointerEvent, handle: Handle) {
    if (event.button !== 0) return;
    if (handle === "move" && !movable) {
      event.stopPropagation();
      return;
    }
    event.preventDefault();
    event.stopPropagation();
    box?.focus({ preventScroll: true });
    (event.currentTarget as HTMLElement).setPointerCapture(event.pointerId);
    drag = { handle, startX: event.clientX, startY: event.clientY, startRect: rect, latest: rect };
  }

  function move(event: PointerEvent) {
    if (!drag) return;
    const dx = (event.clientX - drag.startX) / frame.scale;
    const dy = (event.clientY - drag.startY) / frame.scale;
    drag.latest = dragFrame(drag.startRect, drag.handle, dx, dy, { ...rules(), keepAspect: keepAspect || event.shiftKey });
    onPreview?.(drag.latest);
  }

  function end() {
    if (!drag) return;
    const { latest, startRect } = drag;
    drag = null;
    if (!sameRect(latest, startRect)) onCommit(latest);
  }

  function cancelDrag() {
    if (!drag) return;
    onPreview?.(drag.startRect);
    drag = null;
  }

  function commitKeys() {
    clearTimeout(keyTimer);
    if (keyed && keyedFrom && !sameRect(keyed, keyedFrom)) onCommit(keyed);
    keyed = null;
    keyedFrom = null;
  }

  function onKeyDown(event: KeyboardEvent) {
    if (event.key === "Escape") {
      event.preventDefault();
      event.stopPropagation();
      if (drag) return cancelDrag();
      commitKeys();
      onEscape?.();
      return;
    }
    if ((event.key === "Delete" || event.key === "Backspace") && onDelete) {
      // stopping it here keeps the delete key away from the page commands
      event.preventDefault();
      event.stopPropagation();
      clearTimeout(keyTimer);
      keyed = null;
      keyedFrom = null;
      onDelete();
      return;
    }
    if (event.key === "Enter" && onOpen) {
      event.preventDefault();
      onOpen();
      return;
    }
    const from = keyed ?? rect;
    const next = keyRect
      ? keyRect(event, from)
      : frameAfterKey(from, event.key, { shift: event.shiftKey, alt: event.altKey }, { ...rules(), resizable: handles.length > 0 });
    if (!next) return;
    event.preventDefault();
    // alt with an arrow would otherwise also move the selected pages
    event.stopPropagation();
    if (!movable && !event.altKey && !keyRect) return;
    keyedFrom ??= rect;
    keyed = next;
    onPreview?.(next);
    clearTimeout(keyTimer);
    if (keyCommitDelay <= 0) commitKeys();
    else keyTimer = setTimeout(commitKeys, keyCommitDelay);
  }
</script>

<div
  class="frame-area {name}"
  class:shade
  data-page-ui
  style:left="{frame.left}px"
  style:top="{frame.top}px"
  style:width="{frame.width}px"
  style:height="{frame.height}px"
>
  <!-- the frame is a custom control: "application" tells screen readers
       to pass the arrow keys through to it, and its label says how to use them -->
  <!-- svelte-ignore a11y_no_noninteractive_tabindex, a11y_no_noninteractive_element_interactions -->
  <div
    bind:this={box}
    class="box"
    class:movable
    role="application"
    tabindex="0"
    aria-label={label}
    style:left="{rect.x * frame.scale}px"
    style:top="{rect.y * frame.scale}px"
    style:width="{rect.width * frame.scale}px"
    style:height="{rect.height * frame.scale}px"
    onpointerdown={(event) => start(event, "move")}
    onpointermove={move}
    onpointerup={end}
    onpointercancel={cancelDrag}
    ondblclick={() => onOpen?.()}
    onkeydown={onKeyDown}
  >
    {#each handles as handle (handle)}
      <span
        class="handle {handle}"
        aria-hidden="true"
        style:left="{HANDLE_SPOTS[handle].x * 100}%"
        style:top="{HANDLE_SPOTS[handle].y * 100}%"
        style:cursor={cursorFor(handle)}
        onpointerdown={(event) => start(event, handle)}
        onpointermove={move}
        onpointerup={end}
        onpointercancel={cancelDrag}
      ></span>
    {/each}
  </div>
</div>

<style>
  .frame-area {
    position: absolute;
    pointer-events: none;
  }

  .frame-area.shade {
    overflow: hidden;
  }

  .box {
    position: absolute;
    border: 1px solid var(--accent);
    pointer-events: auto;
    touch-action: none;
    outline: none;
    box-shadow: none;
  }

  .box.movable {
    cursor: move;
  }

  /* everything outside the rectangle is shaded: for crop, what will be hidden */
  .shade .box {
    box-shadow: 0 0 0 100vmax rgb(0 0 0 / 0.38);
  }

  .box:focus-visible {
    outline: 3px solid var(--accent-soft);
    outline-offset: 0;
  }

  .shade .box:focus-visible {
    box-shadow: 0 0 0 100vmax rgb(0 0 0 / 0.38);
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
</style>

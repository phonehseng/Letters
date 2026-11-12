<script lang="ts">
  // the slide being edited: the engine's picture of it, with the objects'
  // frames, the empty placeholders' prompts and the typing fields laid
  // over it. a click only selects (nothing on the slide moves); a drag
  // moves, a handle resizes or turns, with pink guides for what it lines
  // up with (canvasDrag.ts); the release is one operation and one undo
  // step. double-click, Enter or just typing puts the words in the box.
  import type { Handle } from "../writer/resize";
  import CellEditor, { cellAt, nextCell } from "./CellEditor.svelte";
  import { dragOperation, nudgeFor, startMove, startResize, startTurn, updateMove, updateResize, updateTurn, type Drag, type Point } from "./canvasDrag";
  import type { DeckSession } from "./deckSession.svelte";
  import { objectAt, turnPoint, type Box } from "./geometry";
  import ObjectMarks from "./ObjectMarks.svelte";
  import { moveBy, setText, type Operation } from "./operations";
  import SelectionFrame from "./SelectionFrame.svelte";
  import SlidePictureView from "./SlidePictureView.svelte";
  import { pictureWithout, type ObjectView, type SlidePicture } from "./slidesApi";
  import TextEditor from "./TextEditor.svelte";

  interface Props {
    session: DeckSession;
    /** css pixels per point */
    scale: number;
    /** says what happened, for screen readers */
    onAnnounce: (words: string) => void;
  }

  let { session, scale, onAnnounce }: Props = $props();

  type Typing = { kind: "text"; id: number; startWith: string | null } | { kind: "cell"; id: number; row: number; column: number };

  let stage = $state<HTMLDivElement>();
  let hovered = $state<number | null>(null);
  let drag = $state<Drag | null>(null);
  let typing = $state<Typing | null>(null);
  /** the slide drawn without the words being typed */
  let typingPicture = $state<SlidePicture | null>(null);
  /** the field stays until the slide with the new words is on screen, so nothing blinks */
  let closingAt = $state<number | null>(null);
  let shownRevision = $state(0);

  const slide = $derived(session.slide);
  const objects = $derived(session.view?.objects ?? []);
  const selected = $derived(objects.filter((object) => session.selection.includes(object.id)));
  const picture = $derived(typingPicture ?? (slide ? (session.pictures[slide.id] ?? null) : null));
  const unit = navigator.language === "en-US" ? "in" : "cm";

  /** a pointer event as a point on the slide. */
  function slidePoint(event: PointerEvent | MouseEvent): Point {
    const rect = stage!.getBoundingClientRect();
    return { x: (event.clientX - rect.left) / scale, y: (event.clientY - rect.top) / scale };
  }

  const canType = (object: ObjectView) => object.text !== null;

  // --- pointer

  function onPointerDown(event: PointerEvent) {
    if (event.button !== 0 || !slide) return;
    stage?.focus();
    const point = slidePoint(event);
    const hit = objectAt(objects, point);
    if (!hit) {
      session.selection = [];
      return;
    }
    if (event.shiftKey || event.ctrlKey || event.metaKey) {
      const already = session.selection.includes(hit.id);
      session.selection = already ? session.selection.filter((id) => id !== hit.id) : [...session.selection, hit.id];
      return;
    }
    if (!session.selection.includes(hit.id)) session.selection = [hit.id];
    const moving = objects.filter((object) => session.selection.includes(object.id));
    drag = startMove(moving, objects, point);
    stage?.setPointerCapture(event.pointerId);
  }

  function onPointerMove(event: PointerEvent) {
    const point = slidePoint(event);
    if (!drag) {
      hovered = objectAt(objects, point)?.id ?? null;
      return;
    }
    if (drag.kind === "move") drag = updateMove(drag, point, session.deck, scale, event.altKey);
    else if (drag.kind === "resize") drag = updateResize(drag, point, event.shiftKey);
    else drag = updateTurn(drag, point, event.shiftKey);
  }

  async function onPointerUp() {
    const finished = drag;
    drag = null;
    if (!finished || !slide) return;
    const operation = dragOperation(finished, slide.id);
    if (operation) {
      await session.apply(operation);
      return;
    }
    // "Click to add title" means what it says: a click on an empty placeholder types in it
    const clicked = finished.kind === "move" && finished.ids.length === 1 ? objects.find((object) => object.id === finished.ids[0]) : null;
    if (clicked?.text?.prompt) void beginTyping({ kind: "text", id: clicked.id, startWith: null });
  }

  function onDoubleClick(event: MouseEvent) {
    const hit = objectAt(objects, slidePoint(event));
    if (!hit) return;
    if (hit.kind === "table" && hit.table) {
      const local = turnPoint(slidePoint(event), { x: hit.x + hit.width / 2, y: hit.y + hit.height / 2 }, -hit.rotation);
      const cell = cellAt(hit.table, local.x - hit.x, local.y - hit.y);
      if (cell) void beginTyping({ kind: "cell", id: hit.id, ...cell });
    } else if (canType(hit)) {
      void beginTyping({ kind: "text", id: hit.id, startWith: null });
    }
  }

  function grabHandle(handle: Handle, event: PointerEvent) {
    const object = selected[0];
    if (!object) return;
    drag = startResize(object.id, handle, object, slidePoint(event), object.kind === "picture");
    stage?.setPointerCapture(event.pointerId);
  }

  function grabTurn(event: PointerEvent) {
    const object = selected[0];
    if (!object) return;
    drag = startTurn(object.id, object);
    stage?.setPointerCapture(event.pointerId);
  }

  // --- typing

  /** puts the caret in an object's text (after Insert › Text Box, for example). */
  export function startTyping(id: number) {
    void beginTyping({ kind: "text", id, startWith: null });
  }

  async function beginTyping(next: Typing) {
    if (!slide) return;
    session.selection = [next.id];
    if (next.kind === "cell") session.tableCell = { object: next.id, row: next.row, column: next.column };
    typing = next;
    closingAt = null;
    const leftOut = next.kind === "text" ? { type: "text" as const, object: next.id } : { type: "cell" as const, object: next.id, row: next.row, column: next.column };
    typingPicture = (await pictureWithout(session.id, slide.id, leftOut).catch((problem) => session.fail(problem))) ?? null;
  }

  async function finishTyping(operation: Operation | null) {
    const was = typing;
    if (!was) return;
    if (!operation) {
      endTyping();
      return;
    }
    // the field stays while the slide with the new words is drawn and loaded
    closingAt = -1;
    await session.apply(operation, [was.id]);
    typingPicture = null;
    closingAt = slide ? (session.pictures[slide.id]?.revision ?? 0) : 0;
    onAnnounce("Text changed.");
  }

  function endTyping() {
    typing = null;
    typingPicture = null;
    closingAt = null;
    stage?.focus();
  }

  $effect(() => {
    // the new picture is on screen: the field can go
    if (closingAt !== null && closingAt >= 0 && shownRevision >= closingAt) endTyping();
  });

  function textDone(text: string | null) {
    if (!slide || typing?.kind !== "text") return;
    void finishTyping(text === null ? null : setText(slide.id, typing.id, text));
  }

  async function cellDone(text: string | null, move: "next" | "previous" | null) {
    if (!slide || typing?.kind !== "cell") return;
    const { id, row, column } = typing;
    const table = objects.find((object) => object.id === id)?.table;
    const operation: Operation | null = text === null ? null : { op: "setCellText", slide_id: slide.id, object_id: id, row, column, text };
    await finishTyping(operation);
    const next = move && table ? nextCell(table, row, column, move === "previous") : null;
    if (next) void beginTyping({ kind: "cell", id, ...next });
  }

  // --- keys

  function onKeyDown(event: KeyboardEvent) {
    if (!slide || typing) return;
    const nudge = nudgeFor(event.key, event.shiftKey);
    if (nudge && session.selection.length > 0 && !event.ctrlKey && !event.metaKey) {
      event.preventDefault();
      void session.apply(moveBy(slide.id, session.selection, nudge.x, nudge.y));
    } else if ((event.key === "Delete" || event.key === "Backspace") && session.selection.length > 0) {
      event.preventDefault();
      event.stopPropagation();
      void deleteSelection();
    } else if (event.key === "Escape") {
      session.selection = [];
    } else if (event.key === "Tab") {
      event.preventDefault();
      selectNext(event.shiftKey);
    } else if (event.key === "Enter" && selected.length === 1 && canType(selected[0])) {
      event.preventDefault();
      void beginTyping({ kind: "text", id: selected[0].id, startWith: null });
    } else if (event.key.length === 1 && !event.ctrlKey && !event.metaKey && !event.altKey && selected.length === 1 && canType(selected[0])) {
      // typing with a box selected replaces its words, as in powerpoint
      event.preventDefault();
      void beginTyping({ kind: "text", id: selected[0].id, startWith: event.key });
    }
  }

  async function deleteSelection() {
    if (!slide) return;
    const ids = [...session.selection];
    for (const id of ids) await session.apply({ op: "deleteObject", slide_id: slide.id, object_id: id }, []);
    onAnnounce(ids.length === 1 ? "Object deleted." : `${ids.length} objects deleted.`);
  }

  function selectNext(backwards: boolean) {
    if (objects.length === 0) return;
    const at = objects.findIndex((object) => object.id === session.selection[0]);
    const next = (at + (backwards ? -1 : 1) + objects.length) % objects.length;
    session.selection = [objects[next].id];
    onAnnounce(`${objects[next].name} selected.`);
  }

  // --- what the frames and the moving copy show

  function previewOf(object: ObjectView): { box: Box; rotation: number } {
    const base = { box: { x: object.x, y: object.y, width: object.width, height: object.height }, rotation: object.rotation };
    if (!drag) return base;
    if (drag.kind === "move" && drag.ids.includes(object.id)) return { ...base, box: { ...base.box, x: object.x + drag.dx, y: object.y + drag.dy } };
    if (drag.kind === "resize" && drag.id === object.id) return { ...base, box: drag.box };
    if (drag.kind === "turn" && drag.id === object.id) return { ...base, rotation: drag.rotation };
    return base;
  }

  function sizeLabel(box: Box): string | null {
    if (drag?.kind !== "resize") return null;
    const factor = unit === "in" ? 72 : 72 / 2.54;
    const show = (points: number) => (points / factor).toFixed(2).replace(/\.?0+$/, "");
    return `${show(box.width)} × ${show(box.height)} ${unit}`;
  }

  /** the objects whose copy follows the pointer: those being moved or turned */
  function liftedIds(current: Drag | null): number[] {
    if (current?.kind === "move" && current.started) return current.ids;
    if (current?.kind === "turn") return [current.id];
    return [];
  }

  const moving = $derived(objects.filter((object) => liftedIds(drag).includes(object.id)));
  const guides = $derived(drag?.kind === "move" ? drag.guides : []);
  const typedObject = $derived(typing ? (objects.find((object) => object.id === typing!.id) ?? null) : null);
</script>

<!-- the slide is one focusable surface with its own keys (Tab between objects), as a canvas is -->
<!-- svelte-ignore a11y_no_noninteractive_tabindex, a11y_no_noninteractive_element_interactions -->
<div
  bind:this={stage}
  class="stage"
  role="application"
  aria-roledescription="slide"
  aria-label="Slide {session.current + 1}. Click an object to select it, double-click to type in it, Tab to go from object to object."
  tabindex="0"
  style:width="{session.deck.width * scale}px"
  style:height="{session.deck.height * scale}px"
  onpointerdown={onPointerDown}
  onpointermove={onPointerMove}
  onpointerup={onPointerUp}
  onpointerleave={() => (hovered = null)}
  ondblclick={onDoubleClick}
  onkeydown={onKeyDown}
>
  <SlidePictureView {picture} {scale} label="Slide {session.current + 1}" onShown={(revision) => (shownRevision = revision)} />
  <ObjectMarks {objects} {scale} hovered={drag ? null : hovered} editing={typing?.id ?? null} />

  {#each moving as object (object.id)}
    {@const preview = previewOf(object)}
    <!-- a copy of the object's part of the slide follows the pointer; the slide itself waits for the release -->
    <div
      class="lifted"
      style:left="{preview.box.x * scale}px"
      style:top="{preview.box.y * scale}px"
      style:width="{object.width * scale}px"
      style:height="{object.height * scale}px"
      style:transform="rotate({preview.rotation}deg)"
    >
      <div
        class="lifted-slide"
        style:left="{-object.x * scale}px"
        style:top="{-object.y * scale}px"
        style:width="{session.deck.width * scale}px"
        style:height="{session.deck.height * scale}px"
        style:transform-origin="{(object.x + object.width / 2) * scale}px {(object.y + object.height / 2) * scale}px"
        style:transform="rotate({-object.rotation}deg)"
      >
        <SlidePictureView {picture} {scale} />
      </div>
    </div>
  {/each}

  {#each guides as guide, index (index)}
    <div
      class="guide"
      class:vertical={guide.vertical}
      style:left={guide.vertical ? `${guide.position * scale}px` : "0"}
      style:top={guide.vertical ? "0" : `${guide.position * scale}px`}
    ></div>
  {/each}

  {#if !typing}
    {#each selected as object (object.id)}
      {@const preview = previewOf(object)}
      <SelectionFrame
        box={preview.box}
        rotation={preview.rotation}
        {scale}
        resizable={selected.length === 1 && object.kind !== "connector"}
        turnable={selected.length === 1 && object.kind !== "table"}
        sizeLabel={sizeLabel(preview.box)}
        onHandle={grabHandle}
        onTurn={grabTurn}
      />
    {/each}
  {/if}

  {#if typing?.kind === "text" && typedObject?.text}
    {#key typing.id}
      <TextEditor object={typedObject} deckId={session.id} {scale} startWith={typing.startWith} onDone={textDone} />
    {/key}
  {:else if typing?.kind === "cell" && typedObject?.table}
    {#key `${typing.id}/${typing.row}/${typing.column}`}
      <CellEditor object={typedObject} row={typing.row} column={typing.column} {scale} font={session.deck.theme?.minorFont ?? "Calibri"} onDone={cellDone} />
    {/key}
  {/if}
</div>

<style>
  .stage {
    position: relative;
    flex: none;
    overflow: hidden;
    box-shadow: var(--shadow-page);
    outline: none;
    touch-action: none;
  }

  .stage:focus-visible {
    box-shadow:
      0 0 0 3px var(--accent-soft),
      var(--shadow-page);
  }

  .lifted {
    position: absolute;
    z-index: 1;
    overflow: hidden;
    opacity: 0.85;
    pointer-events: none;
  }

  .lifted-slide {
    position: absolute;
  }

  /* the guides are pink, as in the writer and keynote: never the accent, which means "selected" */
  .guide {
    position: absolute;
    z-index: 2;
    width: 100%;
    height: 0;
    border-top: 1px solid rgb(255 45 146);
    pointer-events: none;
  }

  .guide.vertical {
    width: 0;
    height: 100%;
    border-top: 0;
    border-left: 1px solid rgb(255 45 146);
  }
</style>

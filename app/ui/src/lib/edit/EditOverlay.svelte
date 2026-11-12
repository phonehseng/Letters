<script lang="ts">
  // the Edit controls on one page. while Edit Text and Images is on, every
  // paragraph, picture and shape of the page is a target: a paragraph shows
  // a faint outline under the pointer (or the keyboard focus) and opens the
  // in-place editor on a click or Enter; a picture or shape is selected
  // with the shared object frame, and a picture being cropped shows the
  // crop rectangle with its handles. the viewer puts one over every page.
  import ObjectFrame from "../pageTools/ObjectFrame.svelte";
  import { handlesFor, HANDLES } from "../pageTools/objectFrame";
  import type { PageFrame, Point, Rect } from "../pageTools/pagePoints";
  import ShapeLayer from "./ShapeLayer.svelte";
  import { isUpright, visibleBlocks } from "./blocks";
  import type { ImageObject, TextBlock } from "./editApi";
  import EditNotice from "./EditNotice.svelte";
  import type { EditSession } from "./editSession.svelte";
  import InPlaceEditor from "./InPlaceEditor.svelte";

  interface Props {
    session: EditSession;
    frame: PageFrame;
    onFocusDocument: () => void;
  }

  let { session, frame, onFocusDocument }: Props = $props();

  let area = $state<HTMLDivElement>();
  const objects = $derived(session.objects.get(frame.index));
  const active = $derived(session.tool === "edit");
  const blocks = $derived(objects && active ? visibleBlocks(objects) : []);
  const editingHere = $derived(session.editor?.page === frame.index ? session.editor : null);
  const selected = $derived(session.pictures.selected?.page === frame.index ? session.pictures.image : null);
  const cropping = $derived(selected ? session.pictures.cropping : null);

  /** the crop rectangle stays on the picture. */
  function insidePicture(rect: Rect): Rect {
    const image = selected!.rect;
    const x = Math.min(Math.max(rect.x, image.x), image.x + image.width - 1);
    const y = Math.min(Math.max(rect.y, image.y), image.y + image.height - 1);
    const width = Math.min(rect.width, image.x + image.width - x);
    const height = Math.min(rect.height, image.y + image.height - y);
    return { x, y, width, height };
  }

  function pointOf(event: MouseEvent): Point | null {
    const box = area?.getBoundingClientRect();
    if (!box) return null;
    return { x: (event.clientX - box.left) / frame.scale, y: (event.clientY - box.top) / frame.scale };
  }

  function openBlock(event: MouseEvent, block: TextBlock) {
    event.stopPropagation();
    // a click from the keyboard (Enter) has no place: the caret goes at the end
    session.openBlock(frame.index, block, event.detail === 0 ? null : pointOf(event));
  }

  function selectImage(event: PointerEvent, image: ImageObject) {
    event.stopPropagation();
    session.pictures.select(frame.index, image.id);
  }

  function blockLabel(block: TextBlock): string {
    const start = block.text.replace(/\s+/g, " ").trim().slice(0, 60);
    return `Text on page ${frame.index + 1}: “${start}”. Press Enter to edit it.`;
  }

  function frameLabel(image: ImageObject): string {
    const resize = isUpright(image) ? " Alt with the arrows changes its size, keeping its proportions." : "";
    return `Picture on page ${frame.index + 1}. The arrow keys move it, with Shift for 10 points.${resize} Delete deletes it. Escape lets go.`;
  }

  function px(value: number) {
    return `${value * frame.scale}px`;
  }
</script>

<div
  bind:this={area}
  class="edit-page"
  class:active
  data-edit-page={frame.index}
  data-scale={frame.scale}
  style:left="{frame.left}px"
  style:top="{frame.top}px"
  style:width="{frame.width}px"
  style:height="{frame.height}px"
>
  {#if active && objects}
    <!-- shapes first, so text and pictures over them stay on top -->
    <ShapeLayer {session} {frame} part="targets" {area} />
    {#each objects.images as image (image.id)}
      <button
        class="image-target"
        data-page-ui
        tabindex={selected?.id === image.id ? -1 : 0}
        aria-label={`Picture on page ${frame.index + 1}. Press Enter to select it.`}
        style:left={px(image.rect.x)}
        style:top={px(image.rect.y)}
        style:width={px(image.rect.width)}
        style:height={px(image.rect.height)}
        onpointerdown={(event) => selectImage(event, image)}
        onclick={(event) => event.detail === 0 && session.pictures.select(frame.index, image.id)}
      ></button>
    {/each}
    {#each blocks as block (block.id)}
      {#if editingHere?.block?.id !== block.id}
        <button
          class="block-target"
          data-page-ui
          aria-label={blockLabel(block)}
          style:left={px(block.rect.x - 2)}
          style:top={px(block.rect.y - 2)}
          style:width={px(block.rect.width + 4)}
          style:height={px(block.rect.height + 4)}
          onpointerdown={(event) => event.stopPropagation()}
          onclick={(event) => openBlock(event, block)}
        ></button>
      {/if}
    {/each}
  {/if}
</div>

{#if selected && cropping}
  <ObjectFrame
    {frame}
    rect={cropping}
    name="crop"
    label={`Crop of the picture on page ${frame.index + 1}. Drag its edges or corners; the arrows move it. Enter or Done crops, Escape cancels.`}
    movable={true}
    handles={HANDLES}
    shade={true}
    focusOnShow={true}
    onPreview={(rect) => (session.pictures.cropping = insidePicture(rect))}
    onCommit={(rect) => (session.pictures.cropping = insidePicture(rect))}
    onOpen={() => session.pictures.applyCrop()}
    onEscape={() => session.pictures.cancelCrop()}
  />
  <div
    class="image-bar glass"
    role="toolbar"
    aria-label="Crop"
    data-page-ui
    style:left="{frame.left + selected.rect.x * frame.scale}px"
    style:top="{frame.top + Math.max(0, selected.rect.y * frame.scale - 40)}px"
  >
    <button class="button" onclick={() => session.pictures.cancelCrop()}>Cancel</button>
    <button class="button primary" data-crop-done onclick={() => session.pictures.applyCrop()}>Done</button>
  </div>
{:else if selected}
  {#key selected.id}
    <ObjectFrame
      {frame}
      rect={session.pictures.preview ?? selected.rect}
      label={frameLabel(selected)}
      movable={true}
      handles={handlesFor(isUpright(selected), true)}
      keepAspect={true}
      focusOnShow={true}
      onPreview={(rect) => (session.pictures.preview = rect)}
      onCommit={(rect) => session.pictures.place(rect)}
      onDelete={() => session.pictures.delete()}
      onEscape={() => {
        session.pictures.deselect();
        onFocusDocument();
      }}
    />
    <div
      class="image-bar glass"
      role="toolbar"
      aria-label="Picture"
      data-page-ui
      style:left="{frame.left + selected.rect.x * frame.scale}px"
      style:top="{frame.top + Math.max(0, selected.rect.y * frame.scale - 40)}px"
    >
      <button class="button" title="Turn the picture a quarter turn to the left" onclick={() => session.pictures.rotate(-90)}>Turn Left</button>
      <button class="button" title="Turn the picture a quarter turn to the right" onclick={() => session.pictures.rotate(90)}>Turn Right</button>
      <button class="button" title="Show only part of the picture; the rest stays in the file" onclick={() => session.pictures.startCrop()}>Crop</button>
      <button class="button" title="Remove the picture from the page" onclick={() => session.pictures.delete()}>Delete</button>
    </div>
  {/key}
{/if}

<ShapeLayer {session} {frame} part="selection" {area} {onFocusDocument} />

{#if editingHere}
  {#key `${editingHere.block?.id ?? "new"}-${editingHere.position.x}-${editingHere.position.y}`}
    <InPlaceEditor {session} {frame} />
  {/key}
{/if}

{#if session.notice?.page === frame.index}
  <EditNotice {session} {frame} />
{/if}

<style>
  .edit-page {
    position: absolute;
    pointer-events: none;
  }

  .block-target,
  .image-target {
    position: absolute;
    margin: 0;
    padding: 0;
    border: 0;
    border-radius: 2px;
    background: transparent;
    box-shadow: none;
    pointer-events: auto;
    cursor: text;
  }

  .image-target {
    cursor: pointer;
  }

  /* a faint outline shows what a click will edit; the accent is for what is selected */
  .block-target:hover,
  .block-target:focus-visible {
    outline: 1px dashed rgb(0 136 255 / 0.7);
    outline-offset: 0;
    background: rgb(0 136 255 / 0.05);
  }

  .image-target:hover,
  .image-target:focus-visible {
    outline: 1px dashed rgb(0 136 255 / 0.7);
  }

  .image-bar {
    position: absolute;
    z-index: 3;
    display: flex;
    gap: 0.25rem;
    padding: var(--group-padding);
  }

  @media (prefers-contrast: more) {
    .block-target:hover,
    .block-target:focus-visible {
      outline: 2px solid var(--accent);
    }
  }
</style>

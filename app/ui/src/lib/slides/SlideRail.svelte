<script lang="ts">
  // the slide rail on the leading side: a numbered picture of every slide.
  // a click shows a slide (Shift and Ctrl choose several), a drag moves it
  // (the accent drop line shows where it will land), and right-click or
  // the menu key offers New Slide, Duplicate, Hide and Delete. the arrows
  // go from slide to slide and Delete removes the chosen ones.
  import ContextMenu, { type ContextItem } from "../ContextMenu.svelte";
  import { tools } from "../tools";
  import type { DeckSession } from "./deckSession.svelte";
  import { deleteOperations, dropGap, dropOperation, railSelection } from "./railOrder";
  import SlidePictureView from "./SlidePictureView.svelte";

  interface Props {
    session: DeckSession;
    onNewSlide: () => void;
    onAnnounce: (words: string) => void;
  }

  let { session, onNewSlide, onAnnounce }: Props = $props();

  /** the rail's thumbnails are this many css pixels wide */
  const THUMBNAIL_WIDTH = 168;
  const scale = $derived(THUMBNAIL_WIDTH / session.deck.width);

  let list = $state<HTMLOListElement>();
  let anchor = 0;
  let pressed: { index: number; y: number } | null = null;
  let dragging = $state<{ from: number; gap: number } | null>(null);
  let menu = $state<{ x: number; y: number } | null>(null);

  const ids = $derived(session.deck.slides.map((slide) => slide.id));

  // pictures for every slide, a few at a time, as the deck changes
  $effect(() => {
    void session.loadPictures(ids);
  });

  // the slide on screen stays in view, so the keyboard never loses it
  $effect(() => {
    list?.children[session.current]?.scrollIntoView({ block: "nearest" });
  });

  function choose(index: number, event: PointerEvent | KeyboardEvent) {
    const toggle = event.ctrlKey || event.metaKey;
    const selection = railSelection(ids, session.selectedSlides, anchor, index, event.shiftKey, toggle);
    if (!event.shiftKey) anchor = index;
    session.selectedSlides = selection;
    void session.go(index, true);
  }

  function middles(): number[] {
    return [...(list?.children ?? [])].map((item) => {
      const rect = item.getBoundingClientRect();
      return rect.top + rect.height / 2;
    });
  }

  function onPointerDown(index: number, event: PointerEvent) {
    if (event.button !== 0) return;
    choose(index, event);
    pressed = { index, y: event.clientY };
    (event.currentTarget as HTMLElement).setPointerCapture(event.pointerId);
  }

  function onPointerMove(event: PointerEvent) {
    if (!pressed) return;
    // a press becomes a drag past a few pixels, so a click never moves a slide
    if (!dragging && Math.abs(event.clientY - pressed.y) < 4) return;
    dragging = { from: pressed.index, gap: dropGap(event.clientY, middles()) };
  }

  async function onPointerUp() {
    const drop = dragging;
    pressed = null;
    dragging = null;
    if (!drop) return;
    const operation = dropOperation(drop.from, drop.gap);
    if (!operation) return;
    await session.apply(operation);
    onAnnounce(`Slide moved to position ${operation.op === "moveSlide" ? operation.to + 1 : ""}.`);
  }

  async function deleteChosen() {
    for (const operation of deleteOperations(ids, session.selectedSlides)) await session.apply(operation);
    onAnnounce("Slides deleted.");
  }

  async function duplicate() {
    const at = session.current;
    if (await session.apply({ op: "duplicateSlide", index: at })) await session.go(at + 1);
  }

  async function toggleHidden() {
    const slide = session.slide;
    if (!slide) return;
    await session.setHidden(slide.id, !slide.hidden);
    onAnnounce(slide.hidden ? "Slide shown in the slide show again." : "Slide hidden in the slide show.");
  }

  function onKeyDown(event: KeyboardEvent) {
    if (event.key === "ArrowDown" || event.key === "ArrowUp") {
      event.preventDefault();
      const next = Math.max(0, Math.min(ids.length - 1, session.current + (event.key === "ArrowDown" ? 1 : -1)));
      choose(next, event);
      (list?.children[next] as HTMLElement | undefined)?.focus();
    } else if (event.key === "Delete" || event.key === "Backspace") {
      event.preventDefault();
      event.stopPropagation();
      void deleteChosen();
    } else if (event.key === "ContextMenu" || (event.shiftKey && event.key === "F10")) {
      event.preventDefault();
      const rect = (event.target as HTMLElement).getBoundingClientRect();
      menu = { x: rect.left + 24, y: rect.top + 24 };
    }
  }

  const menuItems = $derived<ContextItem[]>([
    { label: tools.sNewSlide.label, run: onNewSlide },
    { label: tools.sDuplicateSlide.label, run: () => void duplicate() },
    { label: session.slide?.hidden ? "Show Slide" : tools.sHideSlide.label, run: () => void toggleHidden() },
    { label: tools.sDeleteSlide.label, separatorBefore: true, disabled: ids.length <= 1, run: () => void deleteChosen() },
  ]);
</script>

<nav class="rail glass" aria-label="Slides">
  <ol bind:this={list} class:dragging={!!dragging} role="listbox" aria-label="Slides" aria-multiselectable="true" onpointermove={onPointerMove} onpointerup={onPointerUp}>
    {#each session.deck.slides as slide, index (slide.id)}
      <li
        class:current={index === session.current}
        class:chosen={session.selectedSlides.includes(slide.id)}
        class:gap-before={dragging?.gap === index}
        class:gap-after={dragging?.gap === ids.length && index === ids.length - 1}
        role="option"
        aria-selected={session.selectedSlides.includes(slide.id)}
        tabindex={index === session.current ? 0 : -1}
        aria-current={index === session.current ? "true" : undefined}
        aria-label="Slide {index + 1}{slide.title ? `, ${slide.title}` : ''}{slide.hidden ? ', hidden' : ''}"
        onpointerdown={(event) => onPointerDown(index, event)}
        oncontextmenu={(event) => {
          event.preventDefault();
          if (!session.selectedSlides.includes(slide.id)) void session.go(index);
          menu = { x: event.clientX, y: event.clientY };
        }}
        onkeydown={onKeyDown}
      >
        <span class="number" class:hidden-slide={slide.hidden}>{index + 1}</span>
        <div class="thumbnail" class:hidden-slide={slide.hidden} style:width="{THUMBNAIL_WIDTH}px" style:height="{session.deck.height * scale}px">
          <SlidePictureView picture={session.pictures[slide.id] ?? null} {scale} />
        </div>
      </li>
    {/each}
  </ol>
</nav>

{#if menu}
  <ContextMenu x={menu.x} y={menu.y} label="Slide" items={menuItems} onClose={() => (menu = null)} />
{/if}

<style>
  .rail {
    display: flex;
    flex-direction: column;
    width: 14rem;
    min-height: 0;
    margin: 0 0 0.5rem 0.5rem;
    overflow: auto;
  }

  ol {
    margin: 0;
    padding: 0.5rem 0.5rem 0.5rem 0.25rem;
    list-style: none;
  }

  li {
    display: flex;
    align-items: flex-start;
    gap: 0.375rem;
    padding: 0.375rem 0.25rem;
    border-radius: var(--radius-control);
    outline: none;
    user-select: none;
  }

  li:focus-visible {
    box-shadow: 0 0 0 2px var(--accent);
  }

  .number {
    width: 1.5rem;
    color: var(--label-secondary);
    font: var(--subheadline);
    font-variant-numeric: tabular-nums;
    text-align: right;
  }

  .thumbnail {
    position: relative;
    flex: none;
    overflow: hidden;
    border-radius: 2px;
    box-shadow: 0 0 0 0.5px var(--separator);
  }

  /* the accent marks the chosen slides, as with the viewer's thumbnails */
  .chosen .thumbnail {
    box-shadow: 0 0 0 1px var(--accent);
  }

  .current .thumbnail {
    box-shadow: 0 0 0 3px var(--accent);
  }

  /* a hidden slide is dimmed and its number struck through, as in powerpoint */
  .thumbnail.hidden-slide {
    opacity: 0.45;
  }

  .number.hidden-slide {
    text-decoration: line-through;
  }

  .gap-before {
    box-shadow: inset 0 2px 0 var(--accent);
  }

  .gap-after {
    box-shadow: inset 0 -2px 0 var(--accent);
  }

  .dragging {
    cursor: grabbing;
  }
</style>

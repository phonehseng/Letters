<script lang="ts">
  // the slide show: the slides full screen on black, driven by powerpoint's
  // keys (slidesCommands.ts) and by clicks, ending on powerpoint's "End of
  // slide show" screen. the presenter view puts the slide, the next one,
  // the notes and a clock in this window; with a second screen the slides
  // themselves go to an audience window there, kept in step by events.
  import { emitTo, listen } from "@tauri-apps/api/event";
  import { onDestroy, onMount, untrack } from "svelte";
  import type { DeckSession } from "./deckSession.svelte";
  import { fitScale } from "./geometry";
  import { showActionForKey } from "./slidesCommands";
  import { closeAudienceWindow, openAudienceWindow, screenCount, setFullScreen } from "./slidesApi";
  import SlidePictureView from "./SlidePictureView.svelte";
  import { elapsed, playList, step } from "./showOrder";

  interface Props {
    session: DeckSession;
    startAt: number;
    presenter: boolean;
    onEnd: (lastIndex: number) => void;
  }

  let { session, startAt, presenter, onEnd }: Props = $props();

  // the show plays the deck as it was when it started, as powerpoint's does
  const startId = untrack(() => session.deck.slides[startAt]?.id ?? null);
  const order = untrack(() => playList(session.deck.slides, startId));
  let position = $state(Math.max(0, order.indexOf(startId ?? "")));
  let ended = $state(false);
  let blank = $state<"black" | "white" | null>(null);
  let typed = "";
  let width = $state(window.innerWidth);
  let height = $state(window.innerHeight);
  let audience = $state<string | null>(null);
  const started = Date.now();
  let now = $state(Date.now());

  const currentId = $derived(order[position] ?? null);
  const nextId = $derived(order[position + 1] ?? null);
  const slideNumber = $derived(currentId ? session.deck.slides.findIndex((slide) => slide.id === currentId) + 1 : 0);
  const notes = $derived(session.deck.slides.find((slide) => slide.id === currentId)?.notes ?? "");
  // what a screen reader hears on each step, as powerpoint's show says it:
  // "Slide 2 of 5, The two stages" (the deck's own titles)
  const slideTitle = $derived(session.deck.slides.find((slide) => slide.id === currentId)?.title ?? "");
  const slideName = $derived(`Slide ${slideNumber} of ${session.deck.slides.length}${slideTitle ? `, ${slideTitle}` : ""}`);
  const spoken = $derived(ended ? "End of slide show. Click or press Space to exit." : blank ? `Screen ${blank === "black" ? "blacked" : "whited"} out.` : slideName);
  let show = $state<HTMLDivElement>();
  // the presenter's own slide takes about two thirds of the width, as in powerpoint
  const mainScale = $derived(presenter ? fitScale(session.deck, { width: width * 0.62, height: height * 0.62 }) : fitScale(session.deck, { width, height }, 8));
  const nextScale = $derived(fitScale(session.deck, { width: width * 0.3, height: height * 0.3 }));

  function act(action: ReturnType<typeof showActionForKey>) {
    if (action === null) return;
    if (typeof action === "object") {
      const index = order.indexOf(session.deck.slides[action.goTo - 1]?.id ?? "");
      if (index >= 0) go(index);
      return;
    }
    if (action === "end") return finish();
    if (action === "black" || action === "white") {
      blank = blank === action ? null : action;
      return;
    }
    blank = null;
    if (action === "first") return go(0);
    if (action === "last") return go(order.length - 1);
    if (ended) {
      if (action === "next") return finish();
      ended = false;
      return;
    }
    const next = step(position, order.length, action === "next" ? 1 : -1);
    if (next === null) ended = true;
    else go(next);
  }

  function go(index: number) {
    ended = false;
    position = index;
  }

  function finish() {
    onEnd(Math.max(0, slideNumber - 1));
  }

  function onKeyDown(event: KeyboardEvent) {
    event.preventDefault();
    event.stopPropagation();
    if (/^[0-9]$/.test(event.key)) {
      typed += event.key;
      return;
    }
    act(showActionForKey(event.key, typed));
    typed = "";
  }

  /** the audience window shows what the presenter shows */
  function tellAudience(target: string | null, payload: { slideId: string | null; blank: string | null; ended: boolean }) {
    if (target) void emitTo(target, "slides-show", payload).catch(() => {});
  }

  $effect(() => tellAudience(audience, { slideId: currentId, blank, ended }));

  // the audience window says when it is ready to listen, and is told where the show is
  $effect(() => {
    const stop = listen("slides-audience-ready", () => tellAudience(audience, { slideId: currentId, blank, ended }));
    return () => void stop.then((unlisten) => unlisten());
  });

  // every slide's picture, so going on never waits
  $effect(() => {
    void session.loadPictures(order);
  });

  onMount(() => {
    // the keyboard and a screen reader start on the show, not the page behind it
    show?.focus();
    const clock = setInterval(() => (now = Date.now()), 1000);
    void (async () => {
      if (presenter && (await screenCount().catch(() => 1)) > 1) {
        audience = await openAudienceWindow(session.id).catch((problem) => session.fail(problem) ?? null);
      }
      if (!audience) await setFullScreen(true).catch(() => {});
    })();
    return () => clearInterval(clock);
  });

  onDestroy(() => {
    if (audience) void closeAudienceWindow(session.id);
    else void setFullScreen(false).catch(() => {});
  });
</script>

<svelte:window onkeydowncapture={onKeyDown} bind:innerWidth={width} bind:innerHeight={height} />

<!-- the show is driven by the window's keys (onKeyDown), so the click needs no key handler of its own -->
<!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_noninteractive_element_interactions -->
<div
  bind:this={show}
  tabindex="-1"
  class="show"
  class:presenter
  role="application"
  aria-roledescription="slide show"
  aria-label="Slide show. Right arrow or Space goes on, left arrow goes back, Escape ends."
  onclick={() => act("next")}
  oncontextmenu={(event) => {
    event.preventDefault();
    act("previous");
  }}
>
  <!-- each step is announced: the slide's number and title, the end, a blanked screen -->
  <p class="visually-hidden" aria-live="polite">{spoken}</p>
  <div class="main">
    {#if ended}
      <p class="end">End of slide show, click to exit.</p>
    {:else if currentId}
      <div class="slide" style:width="{session.deck.width * mainScale}px" style:height="{session.deck.height * mainScale}px">
        <SlidePictureView picture={session.pictures[currentId] ?? null} scale={mainScale} label={slideName} />
      </div>
    {/if}
    {#if blank}<div class="blank {blank}" aria-label="Screen {blank === 'black' ? 'blacked' : 'whited'} out"></div>{/if}
  </div>

  {#if presenter}
    <aside class="side" aria-label="Presenter view">
      <div class="clock" aria-live="off">{elapsed(now - started)}</div>
      <div class="count">Slide {slideNumber} of {session.deck.slides.length}</div>
      <h2>Next</h2>
      {#if nextId}
        <div class="slide small" style:width="{session.deck.width * nextScale}px" style:height="{session.deck.height * nextScale}px">
          <SlidePictureView picture={session.pictures[nextId] ?? null} scale={nextScale} />
        </div>
      {:else}
        <p class="muted">End of slide show</p>
      {/if}
      <h2>Notes</h2>
      <p class="notes">{notes || "No notes on this slide."}</p>
      <p class="muted">{audience ? "The audience sees the slides on the other screen." : "One screen: only you see this view. Connect a projector for the audience window."}</p>
      <div class="buttons">
        <button class="button" onclick={(event) => (event.stopPropagation(), act("previous"))}>Previous</button>
        <button class="button" onclick={(event) => (event.stopPropagation(), act("next"))}>Next</button>
        <button class="button" onclick={(event) => (event.stopPropagation(), act("black"))}>Black Screen</button>
        <button class="button" onclick={(event) => (event.stopPropagation(), finish())}>End Show</button>
      </div>
    </aside>
  {/if}
</div>

<style>
  .show {
    position: fixed;
    inset: 0;
    z-index: 100;
    display: flex;
    background: #000000;
    color: #ffffff;
    cursor: none;
  }

  .show.presenter {
    background: #1c1c1e;
    cursor: default;
  }

  .main {
    position: relative;
    display: grid;
    flex: 1;
    place-items: center;
  }

  .slide {
    position: relative;
    overflow: hidden;
  }

  .blank {
    position: absolute;
    inset: 0;
  }

  .blank.black {
    background: #000000;
  }

  .blank.white {
    background: #ffffff;
  }

  .end {
    font: var(--title);
    color: rgb(255 255 255 / 0.8);
  }

  .side {
    display: flex;
    flex-direction: column;
    gap: 0.5rem;
    width: 34%;
    padding: 1.5rem;
    overflow: auto;
  }

  .clock {
    font: var(--large-title);
    font-variant-numeric: tabular-nums;
  }

  .count,
  .muted {
    color: rgb(255 255 255 / 0.62);
    font: var(--body);
  }

  h2 {
    margin: 0.5rem 0 0;
    font: var(--headline);
  }

  .notes {
    margin: 0;
    font: 400 1.125rem/1.5rem var(--font);
    white-space: pre-wrap;
  }

  .buttons {
    display: flex;
    flex-wrap: wrap;
    gap: 0.5rem;
    margin-top: auto;
  }
</style>

<script lang="ts">
  // the audience's window of a slide show on a second screen: black, with
  // the slide the presenter's window says (an event, "slides-show"), drawn
  // from the same deck the shell holds. its name says which deck
  // ("window-audience-<deck>", see app/shell/src/slides_commands.rs).
  import { emit, listen } from "@tauri-apps/api/event";
  import { getCurrentWindow } from "@tauri-apps/api/window";
  import { fitScale } from "./geometry";
  import { AUDIENCE_PREFIX, deckView, slidePictures, type DeckView, type SlidePicture } from "./slidesApi";
  import SlidePictureView from "./SlidePictureView.svelte";

  const deckId = Number(getCurrentWindow().label.slice(AUDIENCE_PREFIX.length));
  let deck = $state<DeckView | null>(null);
  let picture = $state<SlidePicture | null>(null);
  let blank = $state<"black" | "white" | null>(null);
  let ended = $state(false);
  let width = $state(window.innerWidth);
  let height = $state(window.innerHeight);

  const scale = $derived(deck ? fitScale(deck, { width, height }, 8) : 1);

  $effect(() => {
    void deckView(deckId).then((view) => (deck = view));
    const stop = listen<{ slideId: string | null; blank: "black" | "white" | null; ended: boolean }>("slides-show", async (event) => {
      blank = event.payload.blank;
      ended = event.payload.ended;
      if (!event.payload.slideId) return;
      const [drawn] = await slidePictures(deckId, [event.payload.slideId]);
      if (drawn) picture = drawn;
    });
    // the presenter's window may have spoken before this one could listen
    void stop.then(() => emit("slides-audience-ready", { deck: deckId }));
    return () => {
      void stop.then((unlisten) => unlisten());
    };
  });
</script>

<svelte:window bind:innerWidth={width} bind:innerHeight={height} />

<div class="audience" role="img" aria-label="Slide show for the audience">
  {#if deck && picture && !ended}
    <div class="slide" style:width="{deck.width * scale}px" style:height="{deck.height * scale}px">
      <SlidePictureView {picture} {scale} />
    </div>
  {/if}
  {#if blank}<div class="blank" style:background={blank === "black" ? "#000" : "#fff"}></div>{/if}
</div>

<style>
  .audience {
    position: fixed;
    inset: 0;
    display: grid;
    place-items: center;
    background: #000000;
    cursor: none;
  }

  .slide {
    position: relative;
    overflow: hidden;
  }

  .blank {
    position: absolute;
    inset: 0;
  }
</style>

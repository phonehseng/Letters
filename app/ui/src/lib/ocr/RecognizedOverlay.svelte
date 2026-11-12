<script lang="ts">
  // Show Recognized Text on one page: the words the computer read, drawn
  // where it found them, so a student can see what was read. words it was
  // unsure of are tinted. clicking a word opens a small field to correct
  // it; the page's invisible text is then written again with the fix.
  import { tick } from "svelte";
  import type { PageFrame } from "../pageTools/pagePoints";
  import type { OverlayWord } from "./ocrApi";
  import { DOUBTFUL_WORD, type ScanSession } from "./scanSession.svelte";

  interface Props {
    session: ScanSession;
    frame: PageFrame;
  }

  let { session, frame }: Props = $props();

  const overlay = $derived(session.showText ? session.overlays.get(frame.index) : undefined);
  const correcting = $derived(session.correcting?.page === frame.index ? session.correcting.index : null);
  let field = $state<HTMLInputElement>();
  let typed = $state("");

  const doubtful = (word: OverlayWord) => word.confidence !== null && word.confidence < DOUBTFUL_WORD;

  async function startCorrecting(word: OverlayWord) {
    session.correcting = { page: frame.index, index: word.index };
    typed = word.text;
    await tick();
    field?.focus();
    field?.select();
  }

  function onKeyDown(event: KeyboardEvent, word: OverlayWord) {
    event.stopPropagation();
    if (event.key === "Escape") {
      event.preventDefault();
      session.correcting = null;
    } else if (event.key === "Enter") {
      event.preventDefault();
      if (typed.trim() && typed.trim() !== word.text) session.correctWord(frame.index, word.index, typed);
      else session.correcting = null;
    }
  }

  function label(word: OverlayWord): string {
    const sure = word.confidence === null ? "" : doubtful(word) ? " Letters wasn’t sure of it." : "";
    return `Recognized word “${word.text}”.${sure} Press Enter to correct it.`;
  }
</script>

{#if overlay}
  <div
    class="recognized"
    role="group"
    aria-label={`Recognized text on page ${frame.index + 1}`}
    style:left="{frame.left}px"
    style:top="{frame.top}px"
    style:width="{frame.width}px"
    style:height="{frame.height}px"
  >
    {#each overlay.words as word (word.index)}
      {#if correcting === word.index}
        <input
          bind:this={field}
          bind:value={typed}
          class="field correction"
          data-page-ui
          aria-label={`Correct “${word.text}”. Press Enter to keep the change, Escape to cancel.`}
          style:left="{word.rect.x * frame.scale}px"
          style:top="{word.rect.y * frame.scale}px"
          style:min-width="{word.rect.width * frame.scale}px"
          style:height="{Math.max(word.rect.height * frame.scale, 18)}px"
          style:font-size="{Math.max(word.rect.height * frame.scale * 0.75, 11)}px"
          onkeydown={(event) => onKeyDown(event, word)}
          onblur={() => (session.correcting = null)}
        />
      {:else}
        <button
          class="word"
          class:doubtful={doubtful(word)}
          data-page-ui
          aria-label={label(word)}
          title={word.confidence === null ? word.text : `${word.text} (${Math.round(word.confidence)}% sure)`}
          style:left="{word.rect.x * frame.scale}px"
          style:top="{word.rect.y * frame.scale}px"
          style:width="{word.rect.width * frame.scale}px"
          style:height="{word.rect.height * frame.scale}px"
          style:font-size="{word.rect.height * frame.scale * 0.72}px"
          onpointerdown={(event) => event.stopPropagation()}
          onclick={() => startCorrecting(word)}
        >
          {word.text}
        </button>
      {/if}
    {/each}
  </div>
{/if}

<style>
  .recognized {
    position: absolute;
    z-index: 1;
    pointer-events: none;
  }

  .word,
  .correction {
    position: absolute;
    margin: 0;
    pointer-events: auto;
  }

  /* the words read, over the picture: dark on a light wash, never the accent */
  .word {
    display: flex;
    align-items: center;
    justify-content: center;
    padding: 0;
    overflow: hidden;
    border: 0;
    border-radius: 2px;
    background: rgb(255 255 255 / 0.82);
    box-shadow: inset 0 0 0 0.5px rgb(0 0 0 / 0.25);
    color: rgb(20 20 20);
    font-family: var(--font, system-ui);
    line-height: 1;
    white-space: nowrap;
    cursor: text;
  }

  .word.doubtful {
    background: rgb(255 204 0 / 0.55);
    box-shadow: inset 0 0 0 1px rgb(200 120 0 / 0.8);
  }

  .correction {
    z-index: 2;
    padding: 0 0.25rem;
  }

  @media (prefers-contrast: more) {
    .word {
      background: rgb(255 255 255 / 0.95);
      box-shadow: inset 0 0 0 1px rgb(0 0 0 / 0.6);
    }
  }
</style>

<script lang="ts">
  // Find and Replace for the whole deck: slide text, table cells and
  // speaker notes, in slide order. Enter goes to the next place (the slide
  // is shown and the box selected), Replace changes the one shown, Replace
  // All every one; each replace is one undo step.
  import { tools } from "../tools";
  import type { DeckSession } from "./deckSession.svelte";
  import { findInDeck, type Found } from "./slidesApi";

  interface Props {
    session: DeckSession;
    replacing: boolean;
    onClose: () => void;
    onShowNotes: () => void;
    onAnnounce: (words: string) => void;
  }

  let { session, replacing = $bindable(), onClose, onShowNotes, onAnnounce }: Props = $props();

  let words = $state("");
  let replacement = $state("");
  let matchCase = $state(false);
  let found = $state<Found[]>([]);
  let at = $state(-1);
  let field = $state<HTMLInputElement>();

  $effect(() => {
    field?.focus();
  });

  async function search() {
    found = words ? await findInDeck(session.id, words, matchCase).catch(() => []) : [];
    at = -1;
  }

  async function show(index: number) {
    if (found.length === 0) return;
    at = (index + found.length) % found.length;
    const place = found[at];
    const slideIndex = session.deck.slides.findIndex((slide) => slide.id === place.slideId);
    await session.go(slideIndex);
    if (place.place.type === "notes") onShowNotes();
    else session.selection = [place.place.object];
    onAnnounce(`Match ${at + 1} of ${found.length}, on slide ${place.slideNumber}.`);
  }

  async function replace() {
    const place = found[at];
    if (!place) return show(0);
    if (!(await session.replace(place, words, replacement, matchCase))) return;
    onAnnounce("Replaced.");
    await search();
    await show(Math.max(0, at));
  }

  async function replaceAll() {
    const count = found.length;
    if (count === 0) return;
    if (await session.apply({ op: "findReplace", find: words, replace: replacement, match_case: matchCase })) onAnnounce(`Replaced ${count} place${count === 1 ? "" : "s"}.`);
    await search();
  }

  function onKeyDown(event: KeyboardEvent) {
    if (event.key === "Escape") {
      event.preventDefault();
      onClose();
    } else if (event.key === "Enter" && event.currentTarget === field) {
      event.preventDefault();
      void show(at + (event.shiftKey ? -1 : 1));
    }
  }
</script>

<div class="find glass" role="search" aria-label={tools.sFind.label}>
  <input bind:this={field} class="field" type="search" placeholder="Find in slides and notes" bind:value={words} oninput={() => void search()} onkeydown={onKeyDown} />
  <span class="count" aria-live="polite">{words ? (found.length === 0 ? "Not found" : at >= 0 ? `${at + 1} of ${found.length}` : `${found.length} found`) : ""}</span>
  <button class="button" disabled={found.length === 0} onclick={() => void show(at - 1)} aria-label="Previous match">‹</button>
  <button class="button" disabled={found.length === 0} onclick={() => void show(at + 1)} aria-label="Next match">›</button>
  <label class="check"><input type="checkbox" bind:checked={matchCase} onchange={() => void search()} /> Match Case</label>
  {#if replacing}
    <input class="field" placeholder="Replace with" bind:value={replacement} onkeydown={onKeyDown} />
    <button class="button" disabled={found.length === 0} onclick={() => void replace()}>Replace</button>
    <button class="button" disabled={found.length === 0} onclick={() => void replaceAll()}>Replace All</button>
  {:else}
    <button class="button" onclick={() => (replacing = true)}>Replace…</button>
  {/if}
  <button class="button" onclick={onClose}>Done</button>
</div>

<style>
  .find {
    position: absolute;
    top: calc(var(--toolbar-space) + 0.25rem);
    left: 50%;
    z-index: 6;
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 0.375rem;
    max-width: calc(100% - 2rem);
    padding: 0.375rem 0.5rem;
    transform: translateX(-50%);
  }

  .count {
    min-width: 4.5rem;
    color: var(--label-secondary);
    font: var(--subheadline);
    font-variant-numeric: tabular-nums;
  }
</style>

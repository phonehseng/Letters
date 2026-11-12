<script lang="ts">
  // the speaker notes of the slide on screen, under the slide as in
  // powerpoint. the words are kept when the field is left (one undo step
  // per visit, never one per letter) and go to the presenter view and to
  // notes pages.
  import { tools } from "../tools";
  import type { DeckSession } from "./deckSession.svelte";

  interface Props {
    session: DeckSession;
  }

  let { session }: Props = $props();

  const slide = $derived(session.slide);
  let words = $state("");
  let editingFor: string | null = null;

  // a new slide (or new notes after undo) shows its own words, unless the person is typing
  $effect(() => {
    const notes = slide?.notes ?? "";
    if (editingFor !== slide?.id) words = notes;
  });

  async function keep() {
    const target = slide;
    editingFor = null;
    if (!target || words === target.notes) return;
    await session.apply({ op: "setNotes", slide_id: target.id, text: words });
  }
</script>

<div class="notes glass">
  <textarea
    aria-label="Speaker notes for slide {session.current + 1}"
    placeholder="Click to add notes"
    title={tools.sNotes.meaning}
    bind:value={words}
    onfocus={() => (editingFor = slide?.id ?? null)}
    onblur={() => void keep()}
  ></textarea>
</div>

<style>
  .notes {
    display: flex;
    flex: none;
    height: 7rem;
    margin: 0 0.5rem 0.5rem;
    padding: 0.5rem 0.75rem;
  }

  textarea {
    flex: 1;
    border: 0;
    outline: none;
    background: transparent;
    color: var(--label);
    font: var(--body);
    resize: none;
  }
</style>

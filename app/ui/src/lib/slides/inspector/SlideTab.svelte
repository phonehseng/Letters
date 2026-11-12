<script lang="ts">
  // the Slide tab: the slide's layout (picking one rearranges the slide,
  // keeping its words), whether the slide show skips it, and an honest
  // list of what the file keeps that isn't played or drawn here.
  import { tools } from "../../tools";
  import type { DeckSession } from "../deckSession.svelte";

  interface Props {
    session: DeckSession;
    onAnnounce: (words: string) => void;
  }

  let { session, onAnnounce }: Props = $props();

  const slide = $derived(session.slide);
  const picture = $derived(slide ? session.pictures[slide.id] : null);

  async function applyLayout(layoutId: string) {
    if (!slide || layoutId === slide.layoutId) return;
    const name = session.deck.layouts.find((layout) => layout.id === layoutId)?.name ?? "";
    if (await session.apply({ op: "applyLayout", slide_id: slide.id, layout_id: layoutId })) onAnnounce(`Layout changed to ${name}.`);
  }

  async function toggleHidden() {
    if (!slide) return;
    await session.setHidden(slide.id, !slide.hidden);
  }

  /** what the slide carries that Letters keeps but doesn't play */
  const kept = $derived.by(() => {
    if (!slide) return [];
    const lines: string[] = [];
    if (slide.transition) lines.push(`A “${slide.transition}” transition.`);
    if (slide.animationCount > 0) lines.push(`${slide.animationCount} animation${slide.animationCount === 1 ? "" : "s"}.`);
    return lines;
  });
</script>

{#if slide}
  <section>
    <h3>{tools.sLayout.label}</h3>
    <p class="hint">{tools.sLayout.fileEffect}</p>
    <div class="layouts" role="radiogroup" aria-label="Layouts">
      {#each session.deck.layouts as layout (layout.id)}
        <label class="check">
          <input type="radio" name="slide-layout" checked={layout.id === slide.layoutId} onchange={() => void applyLayout(layout.id)} />
          {layout.name}
        </label>
      {/each}
    </div>
  </section>

  <section>
    <label class="check" title={tools.sHideSlide.tooltip}>
      <input type="checkbox" checked={slide.hidden} onchange={() => void toggleHidden()} />
      {tools.sHideSlide.label}
    </label>
    <p class="hint">{tools.sHideSlide.meaning}</p>
  </section>

  {#if kept.length > 0}
    <section>
      <h3>Kept, Not Played Here</h3>
      {#each kept as line (line)}<p class="hint">{line}</p>{/each}
      <p class="hint">Letters keeps them exactly as they are and saves them back; PowerPoint and LibreOffice play them. The slide show here shows each slide as it ends up.</p>
    </section>
  {/if}

  {#if picture && picture.notes.length > 0}
    <section>
      <h3>Drawn Here With Approximations</h3>
      {#each picture.notes as note (note)}<p class="hint">{note}</p>{/each}
      <p class="hint">Only the picture on screen and the PDF differ; the file keeps everything.</p>
    </section>
  {/if}
{/if}

<style>
  .layouts {
    display: flex;
    flex-direction: column;
    gap: 0.25rem;
  }
</style>

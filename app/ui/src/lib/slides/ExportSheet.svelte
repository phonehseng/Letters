<script lang="ts">
  // Export as PDF: a small sheet with the three ways the engine draws a
  // deck on paper (a slide a page, notes pages, handouts of 2, 3, 4 or 6),
  // then the save panel. the line under the button says what will happen,
  // and afterwards what the drawing had to approximate, in the engine's words.
  import { tools } from "../tools";
  import type { DeckSession } from "./deckSession.svelte";
  import { choosePdfLocation, exportDeckPdf, type PdfLayout } from "./slidesApi";

  interface Props {
    session: DeckSession;
    onClose: () => void;
  }

  let { session, onClose }: Props = $props();

  type Choice = "slides" | "notesPages" | "handouts";
  let choice = $state<Choice>("slides");
  let perPage = $state<2 | 3 | 4 | 6>(3);
  let working = $state(false);
  let result = $state<string | null>(null);

  const layout = $derived<PdfLayout>(choice === "handouts" ? { type: "handouts", perPage } : { type: choice });
  const count = $derived(session.deck.slides.length);
  const pages = $derived(choice === "handouts" ? Math.ceil(count / perPage) : count);
  const line = $derived(`Makes a ${pages}-page PDF of ${count} slide${count === 1 ? "" : "s"}. The presentation doesn’t change.`);

  async function exportNow() {
    const suggested = session.path.replace(/\.(pptx|odp)$/i, choice === "slides" ? ".pdf" : choice === "notesPages" ? " (notes).pdf" : " (handouts).pdf");
    const path = await choosePdfLocation(suggested);
    if (!path) return;
    working = true;
    try {
      const notes = await exportDeckPdf(session.id, layout, path);
      const name = path.split(/[\\/]/).pop();
      result = notes.length === 0 ? `Saved “${name}”. Text is real text you can select and search.` : `Saved “${name}”. ${notes.join(" ")}`;
    } catch (problem) {
      session.fail(problem);
    } finally {
      working = false;
    }
  }
</script>

<div class="backdrop">
  <div class="sheet glass" role="dialog" aria-modal="true" aria-labelledby="export-title">
    <h2 id="export-title">{tools.sExportPdf.label.replace("…", "")}</h2>
    <p class="hint">{tools.sExportPdf.meaning}</p>
    <div class="choices" role="radiogroup" aria-label="What each page shows">
      <label class="check"><input type="radio" bind:group={choice} value="slides" /> Slides, one a page</label>
      <label class="check"><input type="radio" bind:group={choice} value="notesPages" /> Notes Pages, each slide above its notes</label>
      <label class="check"><input type="radio" bind:group={choice} value="handouts" /> Handouts</label>
      {#if choice === "handouts"}
        <label class="setting per-page">
          <span class="caption">Slides a page</span>
          <select class="field" bind:value={perPage}>
            <option value={2}>2</option>
            <option value={3}>3, with lines for writing</option>
            <option value={4}>4</option>
            <option value={6}>6</option>
          </select>
        </label>
      {/if}
    </div>
    {#if result}
      <p class="result" role="status">{result}</p>
    {/if}
    <div class="buttons">
      <button class="button" onclick={onClose}>{result ? "Done" : "Cancel"}</button>
      <button class="button primary" disabled={working} onclick={() => void exportNow()}>{working ? "Exporting…" : "Export…"}</button>
    </div>
    <p class="hint">{line}</p>
  </div>
</div>

<style>
  .backdrop {
    position: absolute;
    inset: 0;
    z-index: 20;
    display: grid;
    place-items: start center;
    padding-top: 5rem;
    background: rgb(0 0 0 / 0.12);
  }

  .sheet {
    display: flex;
    flex-direction: column;
    gap: 0.625rem;
    width: min(26rem, calc(100% - 2rem));
    padding: 1rem 1.25rem;
  }

  h2 {
    margin: 0;
    font: var(--title);
  }

  .choices {
    display: flex;
    flex-direction: column;
    gap: 0.375rem;
  }

  .per-page {
    margin-left: 1.5rem;
  }

  .buttons {
    display: flex;
    justify-content: flex-end;
    gap: 0.5rem;
  }

  .result {
    margin: 0;
    font: var(--body);
  }
</style>

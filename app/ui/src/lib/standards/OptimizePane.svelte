<script lang="ts">
  // Prepare & Check > Make Smaller: a smaller copy that looks the same.
  // what each measure saved, and the engine's list of what it didn't do
  // (subsetting whole fonts, fast web view, recompressing JPEG pictures),
  // said every time so nobody expects more than happened.
  import type { DocumentState } from "../api";
  import { pathBeside } from "../pageApi";
  import { choosePdfDestination, optimize, type OptimizeReport, type WrittenCopy } from "./standardsApi";
  import { optimizeHeadline, readableSize } from "./standardsWords";

  interface Props {
    document: DocumentState;
    run: (work: () => Promise<void>) => Promise<void>;
    busy: boolean;
  }

  let { document: doc, run, busy }: Props = $props();

  /** pictures sharper than this are more than a screen or an office printer shows */
  const SCREEN_AND_PRINT_PPI = 150;

  let downsample = $state(false);
  let pixelsPerInch = $state(SCREEN_AND_PRINT_PPI);
  let objectStreams = $state(true);
  let written = $state<WrittenCopy<OptimizeReport> | null>(null);

  function makeSmaller() {
    return run(async () => {
      const destination = await choosePdfDestination(pathBeside(doc.path, `${doc.name.replace(/\.pdf$/i, "")} (smaller).pdf`));
      if (!destination) return;
      written = await optimize(doc.id, { maxPixelsPerInch: downsample ? pixelsPerInch : null, objectStreams }, destination);
    });
  }
</script>

<label class="check"><input type="checkbox" bind:checked={downsample} /> Make very sharp pictures less sharp</label>
{#if downsample}
  <label class="setting">
    <span class="caption">Sharpest kept (pixels per inch)</span>
    <input class="field" type="number" min="72" max="1200" step="1" bind:value={pixelsPerInch} />
    <span class="hint">150 looks sharp on screen and on an office printer. Print shops want 300.</span>
  </label>
{/if}
<label class="check"><input type="checkbox" bind:checked={objectStreams} /> Pack the file’s parts tightly</label>
<p class="hint">PDF/A-1 doesn’t allow packed parts; leave this off for a PDF/A-1 file.</p>
<div class="buttons"><button class="button primary" disabled={busy} onclick={makeSmaller}>Save a Smaller Copy…</button></div>

{#if written}
  <section class="result" aria-live="polite">
    <p class="headline">{optimizeHeadline(written.report)}</p>
    <ul>{#each written.report.measures as measure (measure.name)}<li>{measure.name}: {readableSize(Math.max(0, measure.bytesSaved))} saved</li>{/each}</ul>
    <p class="caption">Not done</p>
    <ul>{#each written.report.notDone as reason, index (index)}<li>{reason}</li>{/each}</ul>
    <p class="hint">Saved as {written.path}. This document wasn’t changed.</p>
  </section>
{/if}

<style>
  .buttons {
    display: flex;
    justify-content: flex-end;
  }

  .result {
    display: flex;
    flex-direction: column;
    gap: 0.375rem;
  }

  .result p {
    margin: 0;
  }

  .headline {
    font: var(--headline);
  }

  ul {
    margin: 0;
    padding-left: 1.25rem;
  }
</style>

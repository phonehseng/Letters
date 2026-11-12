<script lang="ts">
  // Prepare & Check > PDF/A: check the document against a level, then
  // convert a copy. every result shows the engine's CHECKER_NOTE as it is:
  // the claim is Letters' own checker's, and veraPDF is the reference.
  import type { DocumentState } from "../api";
  import Choices from "../inspector/Choices.svelte";
  import { pathBeside } from "../pageApi";
  import FindingList from "./FindingList.svelte";
  import { checkPdfa, choosePdfDestination, convertPdfa, type ConversionReport, type PdfaLevel, type PdfaReport, type WrittenCopy } from "./standardsApi";
  import { checkHeadline, conversionHeadline, convertedName, PDFA_HINTS, PDFA_LABELS } from "./standardsWords";

  interface Props {
    document: DocumentState;
    run: (work: () => Promise<void>) => Promise<void>;
    busy: boolean;
  }

  let { document: doc, run, busy }: Props = $props();

  let level = $state<PdfaLevel>("a2b");
  let checked = $state<PdfaReport | null>(null);
  let converted = $state<WrittenCopy<ConversionReport> | null>(null);

  function check() {
    return run(async () => {
      converted = null;
      checked = await checkPdfa(doc.id, level);
    });
  }

  function convert() {
    return run(async () => {
      const destination = await choosePdfDestination(pathBeside(doc.path, convertedName(doc.name, level)));
      if (!destination) return;
      converted = await convertPdfa(doc.id, level, destination);
    });
  }
</script>

<Choices label="Level" options={(Object.keys(PDFA_LABELS) as PdfaLevel[]).map((value) => ({ value, label: PDFA_LABELS[value] }))} bind:value={level} />
<p class="hint">{PDFA_HINTS[level]}</p>
<div class="buttons">
  <button class="button" disabled={busy} onclick={check}>Check</button>
  <button class="button primary" disabled={busy} onclick={convert}>Convert and Save a Copy…</button>
</div>

{#if converted}
  {@const report = converted.report}
  <section class="result" aria-live="polite">
    <p class="headline">{conversionHeadline(report)}</p>
    <p>{report.summary}</p>
    <p class="hint">Saved as {converted.path}. This document wasn’t changed.</p>
    {#if report.changes.length > 0}
      <details>
        <summary class="caption">What changed ({report.changes.length})</summary>
        <ul>{#each report.changes as change, index (index)}<li>{change}</li>{/each}</ul>
      </details>
    {/if}
    {#each report.fonts as font (font.font)}
      <p class="hint">
        {font.font}: {font.embeddedAs ? `embedded as ${font.embeddedAs}${font.widthsDiffer ? ", whose letters are a little wider or narrower, so lines may end in other places" : ""}.` : (font.problem ?? "not embedded.")}
      </p>
    {/each}
    <FindingList findings={report.blockers} label="What still stands in the way" />
    <p class="note">{converted.checkerNote}</p>
  </section>
{:else if checked}
  <section class="result" aria-live="polite">
    <p class="headline">{checkHeadline(checked)}</p>
    <FindingList findings={checked.findings} label="Problems found" />
    <p class="note">{checked.note}</p>
  </section>
{/if}

<style>
  .buttons {
    display: flex;
    justify-content: flex-end;
    gap: 0.5rem;
  }

  .result {
    display: flex;
    flex-direction: column;
    gap: 0.5rem;
  }

  .result p {
    margin: 0;
  }

  .headline {
    font: var(--headline);
  }

  .note {
    color: var(--label-secondary);
    font: var(--subheadline);
  }

  ul {
    margin: 0.25rem 0 0;
    padding-left: 1.25rem;
  }
</style>

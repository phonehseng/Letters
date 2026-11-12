<script lang="ts">
  // Prepare & Check > Check This PDF: the engine's preflight. damaged parts
  // of the file, fonts and whether they are embedded, where the bytes go,
  // and the features other apps and printers show differently. it only reads.
  import type { DocumentState } from "../api";
  import FindingList from "./FindingList.svelte";
  import { preflight, type PreflightReport } from "./standardsApi";
  import { readableSize } from "./standardsWords";

  interface Props {
    document: DocumentState;
    run: (work: () => Promise<void>) => Promise<void>;
    busy: boolean;
  }

  let { document: doc, run, busy }: Props = $props();

  let report = $state<PreflightReport | null>(null);
  const missingFonts = $derived(report?.fonts.filter((font) => !font.embedded) ?? []);

  function check() {
    return run(async () => {
      report = await preflight(doc.id);
    });
  }
</script>

<div class="buttons"><button class="button primary" disabled={busy} onclick={check}>Check This PDF</button></div>
{#if report}
  <section class="result" aria-live="polite">
    <p class="headline">{report.summary}</p>
    <FindingList findings={report.structure} label="Problems with the file itself" />
    <FindingList findings={report.portability} label="What may look different elsewhere" />
    <p class="hint">
      {report.fonts.length} {report.fonts.length === 1 ? "font" : "fonts"}{missingFonts.length > 0 ? `, ${missingFonts.length} not embedded: ${missingFonts.map((font) => font.name).join(", ")}` : ", all embedded"}.
      {report.images.length} {report.images.length === 1 ? "picture" : "pictures"}.
    </p>
    <details>
      <summary class="caption">Where the bytes go</summary>
      <ul>{#each report.size as line (line.part)}<li>{line.part}: {readableSize(line.bytes)}</li>{/each}</ul>
    </details>
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
    gap: 0.5rem;
  }

  .result p {
    margin: 0;
  }

  .headline {
    font: var(--headline);
  }

  ul {
    margin: 0.25rem 0 0;
    padding-left: 1.25rem;
  }
</style>

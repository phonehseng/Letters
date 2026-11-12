<script lang="ts">
  // Prepare & Check > Print Check: pdf/x checks, a report only. converting
  // to pdf/x needs the print shop's colour profile, so Letters doesn't.
  import type { DocumentState } from "../api";
  import Choices from "../inspector/Choices.svelte";
  import FindingList from "./FindingList.svelte";
  import { checkPdfx, type PdfxLevel, type PdfxReport } from "./standardsApi";
  import { PDFX_HINTS, PDFX_LABELS } from "./standardsWords";

  interface Props {
    document: DocumentState;
    run: (work: () => Promise<void>) => Promise<void>;
    busy: boolean;
  }

  let { document: doc, run, busy }: Props = $props();

  let level = $state<PdfxLevel>("x4");
  let report = $state<PdfxReport | null>(null);

  function check() {
    return run(async () => {
      report = await checkPdfx(doc.id, level);
    });
  }
</script>

<Choices label="Level" options={(Object.keys(PDFX_LABELS) as PdfxLevel[]).map((value) => ({ value, label: PDFX_LABELS[value] }))} bind:value={level} />
<p class="hint">{PDFX_HINTS[level]}</p>
<div class="buttons"><button class="button primary" disabled={busy} onclick={check}>Check for Printing</button></div>
{#if report}
  <section class="result" aria-live="polite">
    <p class="headline">
      {report.passes ? `Letters’ print checks found nothing against ${PDFX_LABELS[report.level]}.` : `${report.findings.length} ${report.findings.length === 1 ? "problem" : "problems"} against ${PDFX_LABELS[report.level]}.`}
    </p>
    <FindingList findings={report.findings} label="Print problems" />
    {#if report.spotColours.length > 0}<p class="hint">Spot colours for the print shop: {report.spotColours.join(", ")}.</p>{/if}
    <p class="note">{report.note}</p>
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

  .note {
    color: var(--label-secondary);
    font: var(--subheadline);
  }
</style>

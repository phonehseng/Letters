<script lang="ts">
  // Signatures: every digital signature in the document, as the engine
  // checked it: who signed, when, and whether the document is unchanged
  // since (valid), changed after signing, or the signature is damaged.
  // the engine's note says plainly that Letters doesn't check trust lists.
  import { isSelfSigned, signatureStatus, signerName, STATUS_LABELS, statusSentence } from "../../forms/signatures";
  import type { ToolContext } from "../../inspector";
  import ToolSection from "../ToolSection.svelte";

  interface Props {
    context: ToolContext;
    open: boolean;
    onToggle: () => void;
  }

  let { context, open, onToggle }: Props = $props();

  const session = $derived(context.fillSign!);
  const reports = $derived(session.signatures);
</script>

<ToolSection tool="signatures" {open} {onToggle}>
  {#if reports.length === 0}
    <p class="hint">This document has no digital signatures.</p>
  {:else}
    <ul class="signatures" aria-label="Signatures in this document">
      {#each reports as report (report.field)}
        {@const status = signatureStatus(report)}
        <li>
          <div class="top">
            <span class="who">{signerName(report)}</span>
            <span class="status {status}">{STATUS_LABELS[status]}</span>
          </div>
          {#if report.signedAt}<p class="hint">Signed {report.signedAt}</p>{/if}
          <p class="sentence">{statusSentence(report)}</p>
          {#if report.reason}<p class="hint">Reason: {report.reason}</p>{/if}
          {#if report.location}<p class="hint">Place: {report.location}</p>{/if}
          {#if isSelfSigned(report)}
            <p class="hint">Self-signed: the signer made this certificate, so nobody vouches for the name.</p>
          {/if}
          {#if report.problems.length > 0 && status !== "valid"}
            <details>
              <summary class="hint">Details</summary>
              {#each report.problems as problem, index (index)}<p class="hint">{problem}</p>{/each}
            </details>
          {/if}
        </li>
      {/each}
    </ul>
  {/if}
  {#if session.overview}<p class="hint">{session.overview.trustNote}</p>{/if}
</ToolSection>

<style>
  .signatures {
    display: flex;
    flex-direction: column;
    gap: 0.5rem;
    margin: 0;
    padding: 0;
    list-style: none;
  }

  li {
    display: flex;
    flex-direction: column;
    gap: 0.125rem;
    padding: 0.5rem;
    border-radius: var(--radius-small);
    background: var(--control);
  }

  .top {
    display: flex;
    align-items: baseline;
    justify-content: space-between;
    gap: 0.5rem;
  }

  .who {
    font: var(--headline);
  }

  .status {
    font: var(--subheadline);
    font-weight: 600;
  }

  .status.valid {
    color: rgb(30 130 70);
  }

  .status.changed {
    color: rgb(190 110 0);
  }

  .status.damaged {
    color: var(--danger);
  }

  .sentence {
    margin: 0;
  }

  details p {
    margin-top: 0.25rem;
  }
</style>

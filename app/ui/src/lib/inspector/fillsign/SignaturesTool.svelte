<script lang="ts">
  // Verify Signatures: every digital signature and document time stamp,
  // as the engine checked the document shown. for each: who signed, the
  // engine's verdict sentence on what changed since, where its time comes
  // from (a time stamp server, or the signer's clock) with the engine's
  // note, and, for a certified document, what the author allowed. then
  // every revision of the file with the changes it made, sorted into kinds.
  // the engine's note says plainly that Letters doesn't check trust lists.
  import { untrack } from "svelte";
  import { asCommandError } from "../../api";
  import { isSelfSigned, signatureStatus, signerName, STATUS_LABELS, statusSentence } from "../../forms/signatures";
  import { addValidationData, verifyDocument, type DocumentVerification } from "../../forms/signingApi";
  import { CERTIFY_LABELS, CHANGE_KIND_LABELS, timeLine, VERDICT_LABELS } from "../../forms/signingWords";
  import type { ToolContext } from "../../inspector";
  import ToolSection from "../ToolSection.svelte";

  interface Props {
    context: ToolContext;
    open: boolean;
    onToggle: () => void;
  }

  let { context, open, onToggle }: Props = $props();

  const session = $derived(context.fillSign!);
  let report = $state<DocumentVerification | null>(null);

  // checking reads the whole file, so it runs only while the section is open
  $effect(() => {
    const id = context.document.id;
    const revision = context.document.revision;
    if (!open || session.signatures.length === 0) return;
    untrack(() =>
      verifyDocument(id)
        .then((read) => {
          if (revision === context.document.revision) report = read;
        })
        .catch((problem) => context.showError(asCommandError(problem).message)),
    );
  });

  async function storeValidationData() {
    const id = context.document.id;
    const worked = await context.change(() => addValidationData(id));
    if (worked) context.notify("The signers’ certificates are stored in the document, and it is saved.");
  }
</script>

<ToolSection tool="signatures" {open} {onToggle}>
  {#if session.signatures.length === 0}
    <p class="hint">This document has no digital signatures.</p>
  {:else if !report}
    <p class="hint">Checking…</p>
  {:else}
    {#if report.certification}
      <p class="plain"><strong>Certified: {CERTIFY_LABELS[report.certification]}.</strong> {report.certificationNote ?? ""}</p>
    {/if}
    <ul class="signatures" aria-label="Signatures in this document">
      {#each report.signatures as signature, index (signature.field)}
        {@const status = signatureStatus(signature)}
        {@const details = report.details[index]}
        {@const changes = report.changes[index]}
        <li>
          <div class="top">
            <span class="who">{details?.kind === "documentTimestamp" ? "Document time stamp" : signerName(signature)}</span>
            <span class="status {status}">{STATUS_LABELS[status]}</span>
          </div>
          {#if changes}
            <p class="plain verdict {changes.verdict}"><strong>{VERDICT_LABELS[changes.verdict]}.</strong> {changes.sentence}</p>
          {:else}
            <p class="plain">{statusSentence(signature)}</p>
          {/if}
          {#if details}
            <p class="hint">{timeLine(details, signature.signedAt)} {details.timeNote}</p>
          {/if}
          {#if signature.reason}<p class="hint">Reason: {signature.reason}</p>{/if}
          {#if isSelfSigned(signature)}
            <p class="hint">Self-signed: the signer made this certificate, so nobody vouches for the name.</p>
          {/if}
          {#if signature.problems.length > 0 && status !== "valid"}
            <details>
              <summary class="hint">Details</summary>
              {#each signature.problems as problem, problemIndex (problemIndex)}<p class="hint">{problem}</p>{/each}
            </details>
          {/if}
        </li>
      {/each}
    </ul>

    <details class="revisions">
      <summary class="caption">Revisions ({report.revisions.length})</summary>
      <ol>
        {#each report.revisions as revision (revision.index)}
          <li>
            <span class="caption">{revision.index === 0 ? "The original file" : `Revision ${revision.index + 1}`}</span>
            {#each revision.changes as change, changeIndex (changeIndex)}
              <p class="hint"><strong>{CHANGE_KIND_LABELS[change.kind]}:</strong> {change.description}</p>
            {/each}
          </li>
        {/each}
      </ol>
    </details>
    <div class="row">
      <button class="button" onclick={storeValidationData} title="Store the certificates the signatures carry, so the document can be checked years from now. Nothing is fetched.">
        Add Validation Data
      </button>
    </div>
    <p class="hint">{report.trustNote}</p>
  {/if}
</ToolSection>

<style>
  .signatures,
  ol {
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

  .status,
  .verdict strong {
    font: var(--subheadline);
    font-weight: 600;
  }

  .status.valid,
  .verdict.unchanged strong,
  .verdict.asIntended strong {
    color: rgb(30 130 70);
  }

  .status.changed,
  .verdict.noLimits strong {
    color: rgb(190 110 0);
  }

  .status.damaged,
  .verdict.notAllowed strong {
    color: var(--danger);
  }

  .plain {
    margin: 0;
  }

  .revisions ol {
    margin-top: 0.375rem;
  }

  .row {
    display: flex;
    justify-content: flex-end;
  }

  details p {
    margin-top: 0.25rem;
  }
</style>

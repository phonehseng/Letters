<script lang="ts">
  // Audit Trail: the record Letters keeps inside the PDF of what happened
  // to it in Letters (prepared, signed, saved, time stamped), each entry
  // checked by the engine: its hash, its link to the one before, and
  // whether a later signature seals it. the engine's note about what the
  // trail can't know is shown as it is, wherever the trail is.
  //
  // Add Audit Pages puts readable pages listing the entries at the end.
  // pages are page content, which a certification forbids, so they are
  // offered before the document is certified, and refused after.
  import { untrack } from "svelte";
  import { asCommandError } from "../../api";
  import { addAuditPages, appendAuditEntry, auditTrail, type AuditTrailReport } from "../../forms/signingApi";
  import { auditEventText } from "../../forms/signingWords";
  import type { ToolContext } from "../../inspector";
  import ToolSection from "../ToolSection.svelte";

  interface Props {
    context: ToolContext;
    open: boolean;
    onToggle: () => void;
  }

  let { context, open, onToggle }: Props = $props();

  const session = $derived(context.fillSign!);
  let report = $state<AuditTrailReport | null>(null);
  const hasTrail = $derived(report !== null && report.verdict !== "noTrail");
  // new pages after a signature show as a change made since, and a
  // certification forbids them, so the pages are offered only before signing
  const signed = $derived(session.signatures.length > 0);

  $effect(() => {
    const id = context.document.id;
    const revision = context.document.revision;
    if (!open) return;
    untrack(() =>
      auditTrail(id)
        .then((read) => {
          if (revision === context.document.revision) report = read;
        })
        .catch((problem) => context.showError(asCommandError(problem).message)),
    );
  });

  async function run() {
    const id = context.document.id;
    if (!hasTrail) {
      const worked = await context.change(() => appendAuditEntry(id, { type: "prepared", note: null }));
      if (worked) context.notify("The audit trail is started and the document saved.");
      return;
    }
    const worked = await context.change(() => addAuditPages(id));
    if (worked) context.notify("Audit pages added at the end and the document saved.");
  }

  const effectLine = $derived(
    !hasTrail
      ? "Starts the trail with an entry saying you prepared this document, and saves it. Signing with “Record it in the audit trail” adds to it."
      : signed
        ? "This document is signed, so new pages would show as a change after the signature. Add audit pages before signing or certifying."
        : "Adds pages at the end that list every entry, and saves. Add them before certifying: a certification doesn’t allow new pages.",
  );
</script>

<ToolSection tool="auditTrail" {open} {onToggle} actionLabel={hasTrail ? "Add Audit Pages" : "Start Audit Trail"} ready={report !== null && !(hasTrail && signed)} effect={effectLine} onAction={run}>
  {#if report}
    <p class="verdict {report.verdict}"><strong>{report.verdict === "intact" ? "Intact" : report.verdict === "broken" ? "Broken" : "No audit trail"}.</strong> {report.summary}</p>
    {#each report.problems as problem, index (index)}<p class="note" role="alert">{problem}</p>{/each}
    {#if report.entries.length > 0}
      <ol class="entries" aria-label="Audit trail entries">
        {#each report.entries as checked (checked.entry.sequence)}
          {@const sound = checked.hashMatches && checked.chainIntact && checked.documentMatches}
          <li>
            <span class="what">{checked.entry.sequence}. {auditEventText(checked.entry.event)}</span>
            <span class="hint">{checked.entry.actor}, {checked.entry.time}</span>
            <span class="seal {sound ? (checked.sealed ? 'sealed' : 'open') : 'broken'}">
              {!sound ? "Doesn’t check out" : checked.sealed ? "Sealed by a later signature" : "Not sealed yet: the next signature seals it"}
            </span>
          </li>
        {/each}
      </ol>
    {/if}
    <!-- the engine's note, word for word -->
    <p class="hint">{report.note}</p>
  {/if}
</ToolSection>

<style>
  .entries {
    display: flex;
    flex-direction: column;
    gap: 0.375rem;
    margin: 0;
    padding: 0;
    list-style: none;
  }

  li {
    display: flex;
    flex-direction: column;
    gap: 0.125rem;
    padding: 0.375rem 0.5rem;
    border-radius: var(--radius-small);
    background: var(--control);
  }

  .verdict,
  .note {
    margin: 0;
  }

  .verdict.intact strong,
  .seal.sealed {
    color: rgb(30 130 70);
  }

  .verdict.broken strong,
  .seal.broken {
    color: var(--danger);
  }

  .seal {
    font: var(--subheadline);
  }

  .seal.open {
    color: rgb(190 110 0);
  }
</style>

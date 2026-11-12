<script lang="ts">
  // one finding in the list: its title and where it is; opened, the plain
  // explanation, why it matters, what was found, and its buttons: Show on
  // Page (outlines the spot) and Fix when the engine offers one.
  import type { Finding } from "../../a11y/a11yApi";
  import type { A11ySession } from "../../a11y/a11ySession.svelte";
  import { findingKey, hasSpot, isFixableHere, whereText } from "../../a11y/findings";
  import { tooltipFor } from "../../tools";

  interface Props {
    finding: Finding;
    session: A11ySession;
    expanded: boolean;
    onToggle: () => void;
    onFix: () => void;
  }

  let { finding, session, expanded, onToggle, onFix }: Props = $props();

  const id = $derived(`finding-${findingKey(finding).replace(/[^a-z0-9]/gi, "-")}`);

  function fix() {
    session.startWizard();
    const wizard = session.wizard;
    if (wizard) {
      const index = wizard.steps.findIndex((step) => step.key === findingKey(finding));
      if (index >= 0) session.wizard = { ...wizard, index };
    }
    onFix();
  }
</script>

<li class="finding" class:expanded>
  <button class="summary" aria-expanded={expanded} aria-controls={id} onclick={onToggle}>
    <span class="title">{finding.title}</span>
    <span class="where">{whereText(finding)}</span>
  </button>
  {#if expanded}
    <div class="body" {id}>
      <p>{finding.explanation}</p>
      <p><strong>Why it matters:</strong> {finding.whyItMatters}</p>
      {#if finding.details}
        <p class="details">{finding.details}</p>
      {/if}
      <p class="hint">{finding.fixDescription}</p>
      {#if finding.pdfua}
        <p class="hint">{finding.pdfua}</p>
      {/if}
      <div class="buttons">
        {#if hasSpot(finding)}
          <button class="button" title={tooltipFor("showOnPage")} onclick={() => session.show(finding)}>Show on Page</button>
        {/if}
        {#if isFixableHere(finding)}
          <button class="button" onclick={fix}>Fix…</button>
        {:else if finding.fix?.kind === "recogniseText"}
          <p class="hint">Use text recognition on these pages first, then check again.</p>
        {/if}
      </div>
    </div>
  {/if}
</li>

<style>
  .finding {
    border-radius: var(--radius-small);
    background: var(--control);
  }

  .summary {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 0.125rem;
    width: 100%;
    min-height: 2.75rem;
    padding: 0.375rem 0.625rem;
    border: 0;
    border-radius: var(--radius-small);
    background: transparent;
    text-align: left;
  }

  .summary:hover {
    background: var(--control-hover);
  }

  .title {
    font-weight: 600;
  }

  .where {
    color: var(--label-secondary);
    font: var(--subheadline);
  }

  .body {
    display: flex;
    flex-direction: column;
    gap: 0.375rem;
    padding: 0 0.625rem 0.625rem;
  }

  .body p {
    margin: 0;
    line-height: 1.125rem;
  }

  .details {
    color: var(--label-secondary);
  }

  .buttons {
    display: flex;
    flex-wrap: wrap;
    gap: 0.375rem;
    justify-content: flex-end;
  }
</style>

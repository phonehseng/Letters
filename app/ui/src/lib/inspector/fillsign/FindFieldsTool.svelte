<script lang="ts">
  // Find Fields: suggestions for a flat form, from the lines, boxes and
  // labels drawn on its pages. they show on the page as dashed outlines
  // with their labels; the person accepts all, or goes through them
  // (accept, reject, move and resize on the page, change the kind here),
  // and Add Fields makes the accepted ones real fields in one undo step.
  // it is only a suggestion, and a scanned page gives none (that needs
  // text recognition, a later stage).
  import { findEffect } from "../../forms/fillSignEffects";
  import type { DetectedKind } from "../../forms/formsApi";
  import { counts, KIND_LABELS, proposalLabel } from "../../forms/proposals";
  import type { ToolContext } from "../../inspector";
  import Choices from "../Choices.svelte";
  import ToolSection from "../ToolSection.svelte";

  interface Props {
    context: ToolContext;
    open: boolean;
    onToggle: () => void;
  }

  let { context, open, onToggle }: Props = $props();

  const session = $derived(context.fillSign!);
  const pageCount = $derived(context.document.pages.length);
  let scope = $state<"page" | "all">("page");
  let searched = $state(false);
  const list = $derived(session.proposals);
  const tally = $derived(counts(list));
  const KINDS = Object.entries(KIND_LABELS) as [DetectedKind, string][];

  async function act() {
    if (list.length > 0) {
      await session.addAcceptedFields();
      searched = false;
      return;
    }
    const pages = scope === "page" ? [context.currentPage] : Array.from({ length: pageCount }, (_, index) => index);
    await session.findFields(pages);
    searched = true;
  }
</script>

<ToolSection
  tool="findFields"
  {open}
  {onToggle}
  actionLabel={list.length > 0 ? "Add Fields" : "Find Fields"}
  ready={list.length === 0 || tally.accepted > 0}
  effect={findEffect(list, scope, context.currentPage, pageCount)}
  onAction={act}
>
  {#if list.length === 0}
    <Choices
      label="Look on"
      options={[
        { value: "page", label: `Page ${context.currentPage + 1}` },
        { value: "all", label: "All Pages" },
      ]}
      bind:value={scope}
    />
    {#if searched}
      <p class="hint" role="status">
        No places to write were found. If the page is a scan, Letters can’t see its lines yet; use Add Field to draw the fields by hand.
      </p>
    {/if}
  {:else}
    <p class="hint" role="status">
      {list.length} suggested: {tally.accepted} accepted, {tally.rejected} rejected, {tally.pending} not decided. Click one on the page to move or resize it.
    </p>
    <ul class="proposals" aria-label="Suggested fields">
      {#each list as proposal (proposal.id)}
        <li class={proposal.status}>
          <button class="label" title="Show it on the page" onclick={() => session.selectProposal(proposal.id)}>
            {proposalLabel(proposal)}<span class="page"> · page {proposal.page + 1}</span>
          </button>
          <select
            class="field kind"
            aria-label={`Kind of field for ${proposalLabel(proposal)}`}
            value={proposal.kind}
            onchange={(event) => session.setProposalKind(proposal.id, event.currentTarget.value as DetectedKind)}
          >
            {#each KINDS as [kind, label] (kind)}<option value={kind}>{label}</option>{/each}
          </select>
          <span class="decide" role="group" aria-label={`Keep ${proposalLabel(proposal)}?`}>
            <button
              class="button small"
              aria-pressed={proposal.status === "accepted"}
              onclick={() => session.setProposalStatus(proposal.id, proposal.status === "accepted" ? "pending" : "accepted")}>Accept</button
            >
            <button
              class="button small"
              aria-pressed={proposal.status === "rejected"}
              onclick={() => session.setProposalStatus(proposal.id, proposal.status === "rejected" ? "pending" : "rejected")}>Reject</button
            >
          </span>
        </li>
      {/each}
    </ul>
  {/if}
  {#snippet secondary()}
    {#if list.length > 0}
      <button class="button" onclick={() => session.clearProposals()}>Clear</button>
      <button class="button" disabled={tally.pending === 0} onclick={() => session.acceptAllProposals()}>Accept All</button>
    {/if}
  {/snippet}
</ToolSection>

<style>
  .proposals {
    max-height: 18rem;
    margin: 0;
    padding: 0;
    overflow-y: auto;
    list-style: none;
  }

  li {
    display: grid;
    grid-template-columns: 1fr auto;
    gap: 0.25rem 0.375rem;
    padding: 0.375rem 0;
    border-top: 0.5px solid var(--separator);
  }

  li.rejected .label {
    color: var(--label-tertiary);
    text-decoration: line-through;
  }

  .label {
    grid-column: 1 / -1;
    padding: 0;
    border: 0;
    background: transparent;
    font: var(--body);
    text-align: left;
  }

  .page {
    color: var(--label-secondary);
  }

  .kind {
    min-width: 0;
    padding: 0.125rem 0.375rem;
    font: var(--subheadline);
  }

  .decide {
    display: flex;
    gap: 0.25rem;
  }

  .button.small {
    padding: 0.125rem 0.5rem;
    font: var(--subheadline);
    font-weight: 500;
  }

  /* chosen is raised, not coloured: the accent stays for Add Fields */
  .button.small[aria-pressed="true"] {
    background: var(--segment-selected);
    box-shadow: var(--shadow-segment);
  }
</style>

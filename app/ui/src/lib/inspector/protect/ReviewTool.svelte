<script lang="ts">
  // the marked areas, page by page: each can be shown on its page (its
  // frame then takes the keyboard) or removed. the button opens the
  // review, which says exactly what applying will remove.
  import type { ToolContext } from "../../inspector";
  import { markedSentence, markLabel, orderedMarks } from "../../protect/pendingMarks";
  import { reviewEffect } from "../../protect/protectEffects";
  import type { ProtectSession } from "../../protect/protectSession.svelte";
  import ToolSection from "../ToolSection.svelte";

  interface Props {
    context: ToolContext;
    session: ProtectSession;
    open: boolean;
    onToggle: () => void;
  }

  let { context, session, open, onToggle }: Props = $props();

  const pageCount = $derived(context.document.pages.length);
  const marks = $derived(orderedMarks(session.marks));
</script>

<ToolSection
  tool="redactReview"
  {open}
  {onToggle}
  actionLabel="Review…"
  ready={marks.length > 0}
  effect={reviewEffect(marks, pageCount)}
  onAction={() => session.openReview()}
>
  <p class="hint" role="status">{markedSentence(marks, pageCount)}</p>
  {#if marks.length > 0}
    <ul class="marks" aria-label="Areas marked for redaction">
      {#each marks as mark (mark.id)}
        <li class:selected={mark.id === session.selectedMarkId}>
          <span class="what">
            <span class="page">Page {mark.page + 1}</span>
            <span class="label">{markLabel(mark)}</span>
          </span>
          <span class="buttons">
            <button class="button" aria-label={`Show the mark on page ${mark.page + 1}: ${markLabel(mark)}`} onclick={() => session.showMark(mark.id)}>Show</button>
            <button class="button" aria-label={`Remove the mark on page ${mark.page + 1}: ${markLabel(mark)}`} onclick={() => session.removeMark(mark.id)}>Remove</button>
          </span>
        </li>
      {/each}
    </ul>
  {/if}
  {#snippet secondary()}
    {#if marks.length > 1}
      <button class="button" onclick={() => session.clearMarks()}>Remove All</button>
    {/if}
  {/snippet}
</ToolSection>

<style>
  .marks {
    display: flex;
    flex-direction: column;
    gap: 0.25rem;
    max-height: 16rem;
    margin: 0;
    padding: 0;
    overflow: auto;
    list-style: none;
  }

  li {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 0.375rem;
    padding: 0.25rem 0.375rem;
    border-radius: var(--radius-small);
    background: var(--control);
  }

  li.selected {
    box-shadow: inset 0 0 0 1px var(--accent);
  }

  .what {
    display: flex;
    flex: 1 1 8rem;
    flex-direction: column;
    min-width: 0;
  }

  .page {
    color: var(--label-secondary);
    font: var(--subheadline);
  }

  .label {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .buttons {
    display: flex;
    gap: 0.25rem;
  }
</style>

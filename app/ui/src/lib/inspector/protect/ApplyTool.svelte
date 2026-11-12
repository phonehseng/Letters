<script lang="ts">
  // Apply Redactions…: the button opens the confirmation sheet, which
  // says how many areas, that it is permanent once saved, and what it
  // doesn't cover. after applying, the section offers the next step:
  // removing hidden information such as the author and title.
  import type { ToolContext } from "../../inspector";
  import { applyEffect, UNDO_UNTIL_SAVED } from "../../protect/protectEffects";
  import type { ProtectSession } from "../../protect/protectSession.svelte";
  import ToolSection from "../ToolSection.svelte";

  interface Props {
    context: ToolContext;
    session: ProtectSession;
    open: boolean;
    onToggle: () => void;
  }

  let { context, session, open, onToggle }: Props = $props();
</script>

<ToolSection
  tool="redactApply"
  {open}
  {onToggle}
  actionLabel="Apply Redactions…"
  ready={session.marks.length > 0}
  effect={applyEffect(session.marks, context.document.pages.length)}
  onAction={() => session.askToApply()}
>
  <p class="hint">{UNDO_UNTIL_SAVED}</p>
  {#if session.lastApplied > 0 && session.marks.length === 0}
    <div class="next">
      <p class="hint">The author, title and other hidden information may still name someone.</p>
      <button class="button" onclick={() => session.openTool("removeHiddenInfo")}>Also Remove Hidden Information…</button>
    </div>
  {/if}
</ToolSection>

<style>
  .next {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 0.375rem;
    padding: 0.5rem 0.625rem;
    border-radius: var(--radius-small);
    background: var(--control);
  }

  .next p {
    margin: 0;
  }
</style>

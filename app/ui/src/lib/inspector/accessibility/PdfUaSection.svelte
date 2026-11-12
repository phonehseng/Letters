<script lang="ts">
  // Mark as PDF/UA: offered only when the engine says every rule it can
  // check passes, and then only after the person ticks, themselves, each
  // thing PDF/UA needs that no program can judge. the mark never claims
  // more than that.
  import type { A11ySession } from "../../a11y/a11ySession.svelte";
  import ToolSection from "../ToolSection.svelte";

  interface Props {
    session: A11ySession;
    open: boolean;
    onToggle: () => void;
  }

  let { session, open, onToggle }: Props = $props();

  const checks = $derived(session.words?.humanChecks ?? []);
  let ticked = $state<boolean[]>([]);
  const allTicked = $derived(checks.length > 0 && checks.every((_, index) => ticked[index]));
  const allowed = $derived(!!session.report && !session.outOfDate && session.report.machineChecksPass);

  const effectLine = $derived.by(() => {
    if (!session.report) return "Check the document first.";
    if (session.outOfDate) return "The document changed since it was checked. Check it again first.";
    if (!session.report.machineChecksPass) return "Not yet: fix everything under Must Fix first, then check again.";
    if (!allTicked) return "Tick each check once you have done it yourself.";
    return "Writes the PDF/UA mark into the file’s information. It says every rule Letters can check passes and that a person did the checks above. Undo with Ctrl Z.";
  });
</script>

<ToolSection tool="markPdfUa" {open} {onToggle} actionLabel="Mark as PDF/UA" ready={allowed && allTicked} effect={effectLine} onAction={() => session.fix({ kind: "markPdfua" })}>
  {#if session.words}
    <p class="hint">{session.words.pdfUa}</p>
  {/if}
  <fieldset disabled={!allowed}>
    <legend class="caption">Checked by me</legend>
    {#each checks as check, index (index)}
      <label class="check">
        <input type="checkbox" bind:checked={ticked[index]} />
        <span>{check}</span>
      </label>
    {/each}
  </fieldset>
</ToolSection>

<style>
  fieldset {
    display: flex;
    flex-direction: column;
    gap: 0.375rem;
    margin: 0;
    padding: 0;
    border: 0;
  }

  fieldset:disabled {
    color: var(--label-secondary);
  }

  .check {
    align-items: flex-start;
    line-height: 1.125rem;
  }

  .check input {
    margin-top: 0.125rem;
  }
</style>

<script lang="ts">
  // Fix Problems…: the wizard. it walks through the fixable findings one
  // at a time with the small form each needs, says "3 of 11", and has
  // Back and Skip. every fix is one undo step; afterwards the document is
  // checked again and the steps follow what the check finds.
  import type { A11ySession } from "../../a11y/a11ySession.svelte";
  import { whereText } from "../../a11y/findings";
  import { closingText, isFinished, progressText } from "../../a11y/wizard";
  import type { ToolContext } from "../../inspector";
  import ToolSection from "../ToolSection.svelte";
  import Decisions from "./Decisions.svelte";
  import FixStep from "./FixStep.svelte";

  interface Props {
    context: ToolContext;
    session: A11ySession;
    open: boolean;
    onToggle: () => void;
  }

  let { context, session, open, onToggle }: Props = $props();

  const wizard = $derived(session.wizard);
  const step = $derived(session.step);
  const fixable = $derived(session.report ? session.report.findings.filter((finding) => finding.fix && finding.fix.kind !== "recogniseText").length : 0);
  let heading = $state<HTMLElement>();

  const effectLine = $derived.by(() => {
    if (!session.report) return "Check the document first. The fixes come from what the check finds.";
    if (fixable === 0) return "There is nothing Letters can fix here. The checks by hand are still worth doing.";
    return `Goes through ${fixable === 1 ? "the 1 problem" : `the ${fixable} problems`} Letters can fix, one at a time. Each fix is one step for Undo; the file changes only when you choose Save.`;
  });

  // each new step takes the focus at its heading, so a screen reader hears it
  let shownKey = "";
  $effect(() => {
    const key = step ? `${step.key}-${wizard?.index}` : "";
    if (key && key !== shownKey) {
      shownKey = key;
      queueMicrotask(() => heading?.focus());
    }
  });
</script>

<ToolSection
  tool="fixAccessibility"
  {open}
  {onToggle}
  actionLabel={wizard ? undefined : "Start Fixing"}
  ready={!!session.report && fixable > 0 && !session.checking}
  effect={effectLine}
  onAction={() => session.startWizard()}
>
  {#if wizard}
    <div class="wizard" role="group" aria-label="Fix Problems">
      <p class="progress" aria-live="polite">{progressText(wizard)}</p>
      {#if step}
        <h4 tabindex="-1" bind:this={heading}>{step.finding.title}</h4>
        <p class="hint">{whereText(step.finding)}{step.status === "skipped" ? " · skipped" : ""}{step.status === "fixed" ? " · fixed" : ""}</p>
        <p class="explanation">{step.finding.explanation}</p>
        {#key step.key}
          <FixStep {context} {session} {step} />
        {/key}
      {:else if isFinished(wizard)}
        <h4 tabindex="-1" bind:this={heading}>All done</h4>
        <p>{closingText(wizard)}</p>
        <p class="hint">Some things only a person can judge. They are under Check by Hand in the list.</p>
      {/if}
      <Decisions {session} />
      {#if session.lastOutcome}
        <ul class="outcome" aria-label="What the last fix did">
          {#each session.lastOutcome.changes as change, index (index)}
            <li>{change}</li>
          {/each}
        </ul>
      {/if}
      <div class="navigation">
        <button class="button" disabled={wizard.index === 0} onclick={() => session.previousStep()}>Back</button>
        {#if step}
          <button class="button" onclick={() => session.skipStep()}>Skip</button>
        {/if}
        <span class="space"></span>
        <button class="button" onclick={() => session.closeWizard()}>Done</button>
      </div>
    </div>
  {/if}
</ToolSection>

<style>
  .wizard {
    display: flex;
    flex-direction: column;
    gap: 0.5rem;
  }

  .progress {
    margin: 0;
    color: var(--label-secondary);
    font: var(--subheadline);
    font-weight: 600;
  }

  h4 {
    margin: 0;
    font: var(--headline);
    outline: none;
  }

  .explanation {
    margin: 0;
    line-height: 1.125rem;
  }

  .outcome {
    margin: 0;
    padding: 0.5rem 0.625rem 0.5rem 1.5rem;
    border-radius: var(--radius-small);
    background: var(--control);
    color: var(--label-secondary);
    font: var(--subheadline);
  }

  .navigation {
    display: flex;
    gap: 0.375rem;
  }

  .space {
    flex: 1;
  }
</style>

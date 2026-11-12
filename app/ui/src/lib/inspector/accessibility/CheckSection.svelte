<script lang="ts">
  // Check This Document: runs the checker, then says the honest score and
  // lists what it found as Must Fix, Should Fix and Check by Hand.
  import type { A11ySession } from "../../a11y/a11ySession.svelte";
  import { countsLine, groupFindings } from "../../a11y/findings";
  import type { ToolContext } from "../../inspector";
  import ToolSection from "../ToolSection.svelte";
  import FindingItem from "./FindingItem.svelte";

  interface Props {
    context: ToolContext;
    session: A11ySession;
    open: boolean;
    onToggle: () => void;
    /** the person pressed Fix on a finding: the wizard opens at it */
    onFix: () => void;
  }

  let { context, session, open, onToggle, onFix }: Props = $props();

  const report = $derived(session.report);
  const groups = $derived(report ? groupFindings(report.findings) : []);
  let expanded = $state<string | null>(null);

  const effectLine = $derived.by(() => {
    if (session.checking) return "Checking… Pages are drawn to measure contrast, so a long document takes a moment.";
    if (session.outOfDate) return "The document changed since it was checked. Check it again to see where it stands.";
    if (!report) return `Checks tags, reading order, alt text, contrast, language, tables and more, aimed at PDF/UA. Nothing in ${context.document.name} changes.`;
    return "Nothing changes in the document. Fix Problems… below offers the fixes.";
  });
</script>

<ToolSection
  tool="checkAccessibility"
  {open}
  {onToggle}
  actionLabel={session.checking ? "Checking…" : report ? "Check Again" : "Check This Document"}
  ready={!session.checking}
  effect={effectLine}
  onAction={() => session.check()}
>
  {#if report}
    <div class="score" aria-live="polite">
      <p class="statement">{report.score.statement}</p>
      <p class="hint">{countsLine(report)}</p>
    </div>
    {#each groups as group (group.severity)}
      <section class="group" aria-labelledby={`a11y-group-${group.severity}`}>
        <h4 id={`a11y-group-${group.severity}`}>
          {group.title} <span class="count">{group.findings.length}</span>
        </h4>
        <p class="hint">{group.hint}</p>
        <ul>
          {#each group.findings as finding, index (`${finding.id}-${index}`)}
            {@const key = `${group.severity}-${index}`}
            <FindingItem
              {finding}
              {session}
              expanded={expanded === key}
              onToggle={() => (expanded = expanded === key ? null : key)}
              {onFix}
            />
          {/each}
        </ul>
      </section>
    {/each}
  {/if}
</ToolSection>

<style>
  .score {
    display: flex;
    flex-direction: column;
    gap: 0.25rem;
    padding: 0.5rem 0.625rem;
    border-radius: var(--radius-small);
    background: var(--control);
  }

  .statement {
    margin: 0;
    line-height: 1.125rem;
  }

  .group {
    display: flex;
    flex-direction: column;
    gap: 0.25rem;
  }

  h4 {
    display: flex;
    align-items: center;
    gap: 0.375rem;
    margin: 0.25rem 0 0;
    font: var(--headline);
  }

  .count {
    padding: 0 0.375rem;
    border-radius: 0.5rem;
    background: var(--control);
    color: var(--label-secondary);
    font: var(--subheadline);
    font-weight: 600;
  }

  ul {
    display: flex;
    flex-direction: column;
    gap: 0.25rem;
    margin: 0.25rem 0 0;
    padding: 0;
    list-style: none;
  }
</style>

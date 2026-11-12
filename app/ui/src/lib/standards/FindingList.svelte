<script lang="ts">
  // a list of findings from a check: the engine's sentence, where it is,
  // the clause of the standard it comes from, and whether Letters can fix it.
  import type { Finding } from "./standardsApi";
  import { FIX_LABELS, findingPlace } from "./standardsWords";

  interface Props {
    findings: Finding[];
    label: string;
  }

  let { findings, label }: Props = $props();
</script>

{#if findings.length > 0}
  <ul class="findings" aria-label={label}>
    {#each findings as finding, index (index)}
      <li>
        <p class="message">{finding.message}</p>
        <p class="meta">{findingPlace(finding)} · {finding.clause} · {FIX_LABELS[finding.fix]}</p>
      </li>
    {/each}
  </ul>
{/if}

<style>
  .findings {
    display: flex;
    flex-direction: column;
    gap: 0.375rem;
    max-height: 16rem;
    margin: 0;
    padding: 0;
    overflow: auto;
    list-style: none;
  }

  li {
    padding: 0.375rem 0.5rem;
    border-radius: var(--radius-small);
    background: var(--control);
  }

  p {
    margin: 0;
  }

  .meta {
    margin-top: 0.125rem;
    color: var(--label-secondary);
    font: var(--subheadline);
    letter-spacing: var(--subheadline-spacing);
  }
</style>

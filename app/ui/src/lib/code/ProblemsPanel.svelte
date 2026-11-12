<script lang="ts">
  // the Problems panel along the bottom of the code screen: every problem
  // with its line, message, hint and who found it. a click (or Enter) puts
  // the cursor there. the line at the top says who checked the file.
  import { countProblems, countsSentence, problemLabel, type Problem } from "./problems";

  interface Props {
    problems: Problem[];
    checkerLine: string;
    coverage: string;
    checking: boolean;
    onGoTo: (problem: Problem) => void;
    onCheckNow: () => void;
    onClose: () => void;
  }

  let { problems, checkerLine, coverage, checking, onGoTo, onCheckNow, onClose }: Props = $props();
  const counts = $derived(countsSentence(countProblems(problems)));
</script>

<section class="glass problems" aria-label="Problems">
  <header>
    <h2>Problems</h2>
    <span class="summary" role="status">{checking ? "Checking…" : counts}</span>
    <span class="checker" title={coverage}>{checkerLine}</span>
    <button class="button" onclick={onCheckNow} disabled={checking}>Check Now</button>
    <button class="button" onclick={onClose} aria-label="Close Problems">Close</button>
  </header>
  {#if problems.length === 0}
    <p class="empty">{checking ? "Checking the file…" : `${coverage} Nothing was found.`}</p>
  {:else}
    <ul>
      {#each problems as problem, index (index)}
        <li>
          <button class="row" onclick={() => onGoTo(problem)} aria-label={problemLabel(problem)}>
            <span class="severity {problem.severity}" aria-hidden="true">{problem.severity === "error" ? "●" : problem.severity === "warning" ? "▲" : "i"}</span>
            <span class="place">Line {problem.line}:{problem.column}</span>
            <span class="text">
              <span class="message">{problem.message}</span>
              {#if problem.hint}<span class="hint-line">{problem.hint}</span>{/if}
            </span>
            <span class="source">{problem.source}</span>
          </button>
        </li>
      {/each}
    </ul>
  {/if}
</section>

<style>
  .problems {
    display: flex;
    flex-direction: column;
    max-height: 40%;
    min-height: 6rem;
    margin: 0 0.5rem 0.5rem;
    overflow: hidden;
  }

  header {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 0.75rem;
    padding: 0.5rem 0.5rem 0.5rem 0.875rem;
  }

  h2 {
    margin: 0;
    font: var(--headline);
  }

  .summary,
  .checker {
    color: var(--label-secondary);
    font: var(--subheadline);
    letter-spacing: var(--subheadline-spacing);
  }

  .checker {
    flex: 1;
  }

  ul {
    margin: 0;
    padding: 0 0.25rem 0.25rem;
    overflow: auto;
    list-style: none;
  }

  .row {
    display: grid;
    grid-template-columns: 1rem 5.5rem 1fr auto;
    gap: 0.5rem;
    width: 100%;
    padding: 0.375rem 0.5rem;
    border: 0;
    border-radius: var(--radius-small);
    background: transparent;
    color: var(--label);
    font: var(--body);
    text-align: left;
  }

  .row:hover {
    background: var(--control-hover);
  }

  .row:focus-visible {
    outline: 2px solid var(--accent);
  }

  /* shape and colour both say the kind, so colour is never the only sign */
  .severity.error {
    color: var(--danger);
  }

  .severity.warning {
    color: rgb(191 120 0);
  }

  .place,
  .source {
    color: var(--label-secondary);
    font-variant-numeric: tabular-nums;
  }

  .text {
    display: flex;
    flex-direction: column;
    gap: 0.125rem;
  }

  .hint-line {
    color: var(--label-secondary);
  }

  .empty {
    margin: 0;
    padding: 0 0.875rem 0.75rem;
    color: var(--label-secondary);
  }
</style>

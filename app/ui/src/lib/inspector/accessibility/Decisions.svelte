<script lang="ts">
  // what Add Tags Automatically was unsure about: a heading level, a
  // table's header row, a picture behind text. each can be shown on its
  // page; the checks by hand cover what a person should confirm.
  import type { A11ySession } from "../../a11y/a11ySession.svelte";

  interface Props {
    session: A11ySession;
  }

  let { session }: Props = $props();

  const low = $derived(session.decisions.filter((decision) => decision.confidence === "low"));
  const medium = $derived(session.decisions.filter((decision) => decision.confidence === "medium"));
</script>

{#if session.decisions.length > 0}
  <section class="decisions" aria-labelledby="tag-decisions-title">
    <h4 id="tag-decisions-title">Look Over These Tags</h4>
    <p class="hint">
      The tags are a best guess from the layout. {low.length} {low.length === 1 ? "guess needs" : "guesses need"} a look, and {medium.length}
      {medium.length === 1 ? "is" : "are"} probably right.
    </p>
    <ul>
      {#each [...low, ...medium] as decision, index (index)}
        <li>
          <span class="confidence">{decision.confidence === "low" ? "Guess" : "Probably right"}</span>
          <span class="text">Page {decision.page + 1}: {decision.decision}</span>
          {#if decision.rect}
            <button class="button" onclick={() => session.showSpot(decision.page, decision.rect, decision.decision)}>Show on Page</button>
          {/if}
        </li>
      {/each}
    </ul>
  </section>
{/if}

<style>
  .decisions {
    display: flex;
    flex-direction: column;
    gap: 0.25rem;
  }

  h4 {
    margin: 0;
    font: var(--headline);
  }

  ul {
    display: flex;
    flex-direction: column;
    gap: 0.25rem;
    max-height: 16rem;
    margin: 0;
    padding: 0;
    overflow-y: auto;
    list-style: none;
  }

  li {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 0.375rem;
    padding: 0.375rem 0.5rem;
    border-radius: var(--radius-small);
    background: var(--control);
  }

  .confidence {
    color: var(--label-secondary);
    font: var(--subheadline);
    font-weight: 600;
  }

  .text {
    flex: 1 1 10rem;
  }
</style>

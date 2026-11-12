<script lang="ts">
  // the question when re-wrapped text no longer fits its column: how far it
  // runs past what, and the ways out, each with the engine's one sentence.
  // a choice that can't work here is shown dimmed with its reason, so the
  // person sees why. nothing has changed while the sheet shows; Cancel (or
  // Escape) keeps it that way and opens the editor again with the typing.
  import { onMount } from "svelte";
  import { overflowChoices, overflowTitle } from "./changeSentences";
  import type { TextFlow } from "./textFlow.svelte";

  interface Props {
    flow: TextFlow;
  }

  let { flow }: Props = $props();

  const pending = $derived(flow.pending!);
  const choices = $derived(overflowChoices(pending.report));

  let sheet = $state<HTMLDivElement>();

  function onKeyDown(event: KeyboardEvent) {
    if (event.key === "Escape") {
      event.preventDefault();
      event.stopPropagation();
      flow.cancel();
    } else if (event.key === "Tab") {
      const buttons = [...(sheet?.querySelectorAll("button:not(:disabled)") ?? [])] as HTMLButtonElement[];
      const index = buttons.indexOf(window.document.activeElement as HTMLButtonElement);
      const next = buttons[(index + (event.shiftKey ? -1 : 1) + buttons.length) % buttons.length];
      event.preventDefault();
      next?.focus();
    }
  }

  onMount(() => (sheet?.querySelector("button:not(:disabled)") as HTMLButtonElement | null)?.focus());
</script>

<div class="backdrop">
  <div
    class="glass sheet"
    role="dialog"
    aria-modal="true"
    aria-labelledby="overflow-title"
    tabindex="-1"
    data-overflow-sheet
    bind:this={sheet}
    onkeydown={onKeyDown}
  >
    <h2 id="overflow-title">{overflowTitle(pending.report)}</h2>
    <p class="hint">Nothing has changed yet. Choose what to do:</p>
    <ul>
      {#each choices as option (option.choice)}
        <li>
          <button class="button choice" disabled={!option.possible} data-choice={option.choice} onclick={() => flow.choose(option.choice)}>
            <span class="label">{option.label}</span>
            <span class="sentence">{option.sentence}</span>
          </button>
        </li>
      {/each}
    </ul>
    <div class="buttons">
      <button class="button" onclick={() => flow.cancel()}>Cancel</button>
    </div>
  </div>
</div>

<style>
  .backdrop {
    position: fixed;
    inset: 0;
    z-index: 10;
    display: flex;
    justify-content: center;
    align-items: flex-start;
    background: rgb(0 0 0 / 0.12);
  }

  .sheet {
    width: min(28rem, calc(100% - 2rem));
    margin-top: var(--toolbar-space);
    padding: 1.25rem 1.25rem 1rem;
    background: var(--window);
    outline: none;
  }

  h2 {
    margin: 0;
    font: var(--headline);
  }

  .hint {
    margin: 0.375rem 0 0.75rem;
  }

  ul {
    display: flex;
    flex-direction: column;
    gap: 0.375rem;
    margin: 0;
    padding: 0;
    list-style: none;
  }

  .choice {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 0.125rem;
    width: 100%;
    height: auto;
    padding: 0.5rem 0.625rem;
    text-align: left;
  }

  .choice:disabled {
    opacity: 0.5;
  }

  .label {
    font: var(--headline);
  }

  .sentence {
    color: var(--label-secondary);
    font: var(--subheadline);
    line-height: 1rem;
    white-space: normal;
  }

  .buttons {
    display: flex;
    justify-content: flex-end;
    margin-top: 1rem;
  }
</style>

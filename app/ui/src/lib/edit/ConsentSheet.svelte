<script lang="ts">
  // the question before changing a digitally signed file, naming the
  // signer: Edit Anyway makes the change (the signature then shows as
  // invalid), Cancel changes nothing. a sheet like the other questions:
  // Escape cancels, Tab stays inside, and Cancel starts with the focus,
  // since breaking a signature should be a deliberate choice.
  import { onMount } from "svelte";
  import type { ConsentQuestion } from "./consent.svelte";

  interface Props {
    question: ConsentQuestion;
  }

  let { question }: Props = $props();

  let sheet = $state<HTMLDivElement>();
  let cancelButton = $state<HTMLButtonElement>();

  function onKeyDown(event: KeyboardEvent) {
    if (event.key === "Escape") {
      event.preventDefault();
      event.stopPropagation();
      question.answer(false);
    } else if (event.key === "Tab") {
      const buttons = [...(sheet?.querySelectorAll("button") ?? [])];
      const index = buttons.indexOf(window.document.activeElement as HTMLButtonElement);
      const next = buttons[(index + (event.shiftKey ? -1 : 1) + buttons.length) % buttons.length];
      event.preventDefault();
      next?.focus();
    }
  }

  onMount(() => cancelButton?.focus());
</script>

<div class="backdrop">
  <div
    class="glass sheet"
    role="alertdialog"
    aria-modal="true"
    aria-labelledby="edit-consent-title"
    aria-describedby="edit-consent-message"
    tabindex="-1"
    data-edit-consent
    bind:this={sheet}
    onkeydown={onKeyDown}
  >
    <h2 id="edit-consent-title">Edit a signed document?</h2>
    <p id="edit-consent-message">{question.message}</p>
    <div class="buttons">
      <button class="button" onclick={() => question.answer(true)}>Edit Anyway</button>
      <span class="space"></span>
      <button class="button primary" bind:this={cancelButton} onclick={() => question.answer(false)}>Cancel</button>
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
    width: min(26rem, calc(100% - 2rem));
    margin-top: var(--toolbar-space);
    padding: 1.25rem 1.25rem 1rem;
    background: var(--window);
    outline: none;
  }

  h2 {
    margin: 0;
    font: var(--headline);
  }

  p {
    margin: 0.5rem 0 0;
    color: var(--label-secondary);
    line-height: 1.125rem;
  }

  .buttons {
    display: flex;
    flex-wrap: wrap;
    gap: 0.5rem;
    margin-top: 1.25rem;
  }

  .space {
    flex: 1;
  }
</style>

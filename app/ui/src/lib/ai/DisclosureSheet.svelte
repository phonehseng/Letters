<script lang="ts">
  // the question before text leaves this computer: the engine's own
  // sentence ("This will send the text of pages 3–7 (about 2,400 words)
  // to api.example.com."), the start of the text, and Send or Cancel.
  // Cancel has the focus, as in any alert that sends something away. the
  // box only covers this provider and this document, while Letters runs.
  import { onMount } from "svelte";
  import type { Disclosure } from "./aiPanel";

  interface Props {
    disclosure: Disclosure;
    onSend: (remember: boolean) => void;
    onCancel: () => void;
  }

  let { disclosure, onSend, onCancel }: Props = $props();
  let remember = $state(false);
  let cancelButton = $state<HTMLButtonElement>();

  onMount(() => cancelButton?.focus());

  function onKeyDown(event: KeyboardEvent) {
    if (event.key === "Escape") {
      event.stopPropagation();
      onCancel();
    }
  }
</script>

<div class="backdrop">
  <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
  <div class="glass alert" role="alertdialog" aria-modal="true" aria-labelledby="disclosure-title" aria-describedby="disclosure-sentence" tabindex="-1" onkeydown={onKeyDown}>
    <h2 id="disclosure-title">Send to {disclosure.destination}?</h2>
    <p id="disclosure-sentence">{disclosure.sentence}</p>
    <blockquote aria-label="The start of what will be sent">{disclosure.preview}</blockquote>
    <label class="check">
      <input type="checkbox" bind:checked={remember} />
      Don’t ask again for {disclosure.providerName} and this document
    </label>
    <div class="buttons">
      <span class="spacer"></span>
      <button class="button" bind:this={cancelButton} onclick={onCancel}>Cancel</button>
      <button class="button primary" onclick={() => onSend(remember)}>Send</button>
    </div>
  </div>
</div>

<style>
  .backdrop {
    position: fixed;
    inset: 0;
    z-index: 20;
    display: grid;
    place-items: center;
    background: rgb(0 0 0 / 0.15);
  }

  .alert {
    display: flex;
    flex-direction: column;
    gap: 0.625rem;
    width: min(26rem, calc(100% - 2rem));
    padding: 1rem;
    box-shadow: var(--shadow-sheet);
  }

  h2 {
    margin: 0;
    font: var(--headline);
  }

  p {
    margin: 0;
  }

  blockquote {
    margin: 0;
    padding: 0.5rem 0.625rem;
    border-radius: var(--radius-control);
    background: var(--control);
    color: var(--label-secondary);
    font: var(--subheadline);
    letter-spacing: var(--subheadline-spacing);
  }

  .check {
    display: flex;
    align-items: center;
    gap: 0.375rem;
  }

  .buttons {
    display: flex;
    gap: 0.5rem;
  }

  .spacer {
    flex: 1;
  }
</style>

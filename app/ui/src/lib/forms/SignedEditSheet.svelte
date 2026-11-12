<script lang="ts">
  // the question before the first change to a digitally signed document:
  // Save a Copy keeps the signed file as it is and puts the change in a
  // copy; Edit Anyway changes this document (its signature then shows as
  // changed, and Save is refused so the signed file is never rewritten);
  // Cancel makes no change. a sheet, like the unsaved changes question:
  // Enter picks Save a Copy, Escape cancels, Tab stays inside.
  import { onMount } from "svelte";
  import { SIGNED_EDIT_QUESTION, SIGNED_EDIT_TITLE, type GuardChoice } from "./signatures";

  interface Props {
    onChoose: (choice: GuardChoice) => void;
  }

  let { onChoose }: Props = $props();

  let sheet = $state<HTMLDivElement>();
  let copyButton = $state<HTMLButtonElement>();

  function onKeyDown(event: KeyboardEvent) {
    if (event.key === "Escape") {
      event.preventDefault();
      event.stopPropagation();
      onChoose("cancel");
    } else if (event.key === "Tab") {
      const buttons = [...(sheet?.querySelectorAll("button") ?? [])];
      const index = buttons.indexOf(window.document.activeElement as HTMLButtonElement);
      const next = buttons[(index + (event.shiftKey ? -1 : 1) + buttons.length) % buttons.length];
      event.preventDefault();
      next?.focus();
    }
  }

  onMount(() => copyButton?.focus());
</script>

<div class="backdrop">
  <div
    class="glass sheet"
    role="alertdialog"
    aria-modal="true"
    aria-labelledby="signed-edit-title"
    aria-describedby="signed-edit-message"
    tabindex="-1"
    bind:this={sheet}
    onkeydown={onKeyDown}
  >
    <h2 id="signed-edit-title">{SIGNED_EDIT_TITLE}</h2>
    <p id="signed-edit-message">{SIGNED_EDIT_QUESTION}</p>
    <div class="buttons">
      <button class="button" onclick={() => onChoose("editAnyway")}>Edit Anyway</button>
      <span class="space"></span>
      <button class="button" onclick={() => onChoose("cancel")}>Cancel</button>
      <button class="button primary" bind:this={copyButton} onclick={() => onChoose("saveCopy")}>Save a Copy…</button>
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
    animation: drop 0.2s var(--ease);
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

  @keyframes drop {
    from {
      opacity: 0;
      transform: translateY(-0.75rem);
    }
  }
</style>

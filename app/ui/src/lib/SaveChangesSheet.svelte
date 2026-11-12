<script lang="ts">
  // the question asked before unsaved changes would be lost, in the words
  // and button order of a mac: Don't Save on its own at the leading edge,
  // then Cancel and Save, with Save as the prominent button.
  //
  // it is a sheet: it drops from the top of the window and the window
  // behind it can't be used until it is answered. Enter saves, Escape
  // cancels, and Tab moves between the three buttons only.
  import { onMount } from "svelte";
  import { choiceForKey, unsavedChangesQuestion, type SaveChoice } from "./documentState";

  interface Props {
    name: string;
    onChoose: (choice: SaveChoice) => void;
  }

  let { name, onChoose }: Props = $props();

  const question = $derived(unsavedChangesQuestion(name));
  let sheet = $state<HTMLDivElement>();
  let saveButton = $state<HTMLButtonElement>();

  function onKeyDown(event: KeyboardEvent) {
    if (event.key === "Tab") {
      keepFocusInside(event);
      return;
    }
    // Enter on a focused button presses that button, as everywhere else
    const focusedButton = window.document.activeElement instanceof HTMLButtonElement;
    const choice = choiceForKey(event.key);
    if (!choice || (choice === "save" && focusedButton && window.document.activeElement !== saveButton)) return;
    event.preventDefault();
    event.stopPropagation();
    onChoose(choice);
  }

  /** the sheet is modal, so Tab goes round its own buttons */
  function keepFocusInside(event: KeyboardEvent) {
    const buttons = [...(sheet?.querySelectorAll("button") ?? [])];
    const index = buttons.indexOf(window.document.activeElement as HTMLButtonElement);
    const step = event.shiftKey ? -1 : 1;
    const next = buttons[(index + step + buttons.length) % buttons.length];
    event.preventDefault();
    next?.focus();
  }

  onMount(() => saveButton?.focus());
</script>

<!-- the backdrop stops clicks reaching the document while the question is open -->
<div class="backdrop">
  <div
    class="glass sheet"
    role="alertdialog"
    aria-modal="true"
    aria-labelledby="save-changes-title"
    aria-describedby="save-changes-message"
    tabindex="-1"
    bind:this={sheet}
    onkeydown={onKeyDown}
  >
    <h2 id="save-changes-title">{question.title}</h2>
    <p id="save-changes-message">{question.message}</p>
    <div class="buttons">
      <button class="button" onclick={() => onChoose("dont-save")}>Don’t Save</button>
      <span class="space"></span>
      <button class="button" onclick={() => onChoose("cancel")}>Cancel</button>
      <button class="button primary" bind:this={saveButton} onclick={() => onChoose("save")}>Save</button>
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

  /* the sheet hangs just under the toolbar, as on a mac. it is solid,
     not glass: a question this important must be easy to read, with
     nothing showing through it */
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
    gap: 0.5rem;
    margin-top: 1.25rem;
  }

  .space {
    flex: 1;
  }

  .button {
    min-width: 5rem;
  }

  @keyframes drop {
    from {
      opacity: 0;
      transform: translateY(-0.75rem);
    }
  }
</style>

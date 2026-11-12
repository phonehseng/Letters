<script lang="ts" module>
  export type ChangedChoice = "replace" | "save-as" | "cancel";
</script>

<script lang="ts">
  // the question asked when saving would write over a file that another
  // window or program changed since this window opened it. saving over it
  // would throw that change away, so the safe answer, Save As, is the
  // prominent one; Replace sits alone at the leading edge, as Don't Save
  // does in the unsaved-changes sheet, because it is the one that loses work.
  //
  // like that sheet it is modal: Enter saves a copy, Escape cancels, and
  // Tab moves between the three buttons only.
  import { onMount } from "svelte";

  interface Props {
    name: string;
    /** the engine's sentence: what happened and what Save As keeps */
    message: string;
    onChoose: (choice: ChangedChoice) => void;
  }

  let { name, message, onChoose }: Props = $props();

  let sheet = $state<HTMLDivElement>();
  let saveAsButton = $state<HTMLButtonElement>();

  function onKeyDown(event: KeyboardEvent) {
    if (event.key === "Tab") {
      keepFocusInside(event);
      return;
    }
    if (event.key === "Escape") {
      event.preventDefault();
      onChoose("cancel");
    }
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

  onMount(() => saveAsButton?.focus());
</script>

<!-- the backdrop stops clicks reaching the document while the question is open -->
<div class="backdrop">
  <div
    class="glass sheet"
    role="alertdialog"
    aria-modal="true"
    aria-labelledby="changed-on-disk-title"
    aria-describedby="changed-on-disk-message"
    tabindex="-1"
    bind:this={sheet}
    onkeydown={onKeyDown}
  >
    <h2 id="changed-on-disk-title">“{name}” changed since you opened it</h2>
    <p id="changed-on-disk-message">{message}</p>
    <div class="buttons">
      <button class="button" onclick={() => onChoose("replace")}>Replace</button>
      <span class="space"></span>
      <button class="button" onclick={() => onChoose("cancel")}>Cancel</button>
      <button class="button primary" bind:this={saveAsButton} onclick={() => onChoose("save-as")}>Save As…</button>
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

  /* solid, not glass, like the unsaved-changes sheet: an important
     question must be easy to read */
  .sheet {
    width: min(28rem, calc(100% - 2rem));
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

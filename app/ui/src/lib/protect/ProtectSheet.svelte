<script lang="ts">
  // a sheet for the Protect questions, as the unsaved changes sheet is:
  // it hangs under the toolbar, the window behind it waits, Tab goes
  // round its own controls only, Escape cancels, and the button meant to
  // take the focus first has it. it is solid, not glass: a question about
  // removing content for good must be easy to read.
  import { onMount, type Snippet } from "svelte";

  interface Props {
    /** an id for the title and message, unique in the window */
    id: string;
    title: string;
    message?: string;
    /** "alertdialog" for a question, "dialog" for the review */
    role?: "alertdialog" | "dialog";
    wide?: boolean;
    onCancel: () => void;
    children?: Snippet;
    buttons: Snippet;
  }

  let { id, title, message, role = "alertdialog", wide = false, onCancel, children, buttons }: Props = $props();

  let sheet = $state<HTMLDivElement>();

  function focusable(): HTMLElement[] {
    const all = sheet?.querySelectorAll<HTMLElement>("button, input, select, textarea, [tabindex]:not([tabindex='-1'])") ?? [];
    return [...all].filter((element) => !element.hasAttribute("disabled"));
  }

  function onKeyDown(event: KeyboardEvent) {
    if (event.key === "Escape") {
      event.preventDefault();
      event.stopPropagation();
      onCancel();
      return;
    }
    // the window's shortcuts (undo, save...) wait while the sheet is open
    if (event.ctrlKey || event.metaKey) event.stopPropagation();
    if (event.key !== "Tab") return;
    const items = focusable();
    if (items.length === 0) return;
    const index = items.indexOf(window.document.activeElement as HTMLElement);
    const next = items[(index + (event.shiftKey ? -1 : 1) + items.length) % items.length];
    event.preventDefault();
    next.focus();
  }

  onMount(() => {
    const first = sheet?.querySelector<HTMLElement>("[data-autofocus]") ?? focusable()[0];
    first?.focus();
  });
</script>

<div class="backdrop" data-page-ui>
  <div
    class="glass sheet"
    class:wide
    {role}
    aria-modal="true"
    aria-labelledby={`${id}-title`}
    aria-describedby={message ? `${id}-message` : undefined}
    tabindex="-1"
    bind:this={sheet}
    onkeydown={onKeyDown}
  >
    <h2 id={`${id}-title`}>{title}</h2>
    {#if message}
      <p id={`${id}-message`} class="message">{message}</p>
    {/if}
    {@render children?.()}
    <div class="buttons">
      {@render buttons()}
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
    display: flex;
    flex-direction: column;
    gap: 0.75rem;
    width: min(28rem, calc(100% - 2rem));
    max-height: calc(100% - var(--toolbar-space) - 1rem);
    margin-top: var(--toolbar-space);
    padding: 1.25rem 1.25rem 1rem;
    overflow: auto;
    background: var(--window);
    outline: none;
    animation: drop 0.2s var(--ease);
  }

  .sheet.wide {
    width: min(44rem, calc(100% - 2rem));
  }

  h2 {
    margin: 0;
    font: var(--headline);
  }

  .message {
    margin: 0;
    color: var(--label-secondary);
    line-height: 1.125rem;
  }

  .buttons {
    display: flex;
    flex-wrap: wrap;
    justify-content: flex-end;
    gap: 0.5rem;
    margin-top: 0.5rem;
  }

  .buttons :global(.button) {
    min-width: 5rem;
  }

  .buttons :global(.space) {
    flex: 1;
  }

  @keyframes drop {
    from {
      opacity: 0;
      transform: translateY(-0.75rem);
    }
  }

  @media (prefers-reduced-motion: reduce) {
    .sheet {
      animation: none;
    }
  }
</style>

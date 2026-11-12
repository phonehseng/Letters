<script lang="ts">
  // the two small cards under a cell (or the formula bar) while a formula
  // is typed: the functions autocomplete offers, each with a line saying
  // why it was offered, and the hint for the function the cursor is in,
  // with the argument being typed in bold and what it should be.
  import type { CellEditor } from "./cellEditor.svelte";

  interface Props {
    editor: CellEditor;
  }

  let { editor }: Props = $props();

  const why = { name: "", spelling: "close spelling", purpose: "does what you typed" };
  const hint = $derived(editor.hint);
  const argument = $derived(hint ? hint.function.arguments[Math.min(hint.argument, hint.function.arguments.length - 1)] : null);
</script>

<div class="popups">
  {#if editor.completions.length > 0}
    <ul class="glass list" role="listbox" aria-label="Functions">
      {#each editor.completions as completion, index (completion.name)}
        <li role="option" aria-selected={index === editor.highlighted}>
          <button
            class:highlighted={index === editor.highlighted}
            tabindex="-1"
            onpointerdown={(event) => {
              // keeps the focus in the text field while choosing
              event.preventDefault();
              editor.acceptCompletion(index);
            }}
          >
            <strong>{completion.name}</strong>
            {#if why[completion.matchedBy]}<em>{why[completion.matchedBy]}</em>{/if}
            <span>{completion.description}</span>
          </button>
        </li>
      {/each}
      <li class="footnote" aria-hidden="true">Tab puts the function in.</li>
    </ul>
  {:else if hint}
    <div class="glass hint" role="status">
      <div class="signature">
        {hint.function.name}(
        {#each hint.function.arguments as item, index (item.name)}
          {#if index > 0},
          {/if}<span class:current={item === argument}>{item.optional ? `[${item.name}]` : item.name}{item.repeating ? ", …" : ""}</span>
        {/each})
      </div>
      {#if argument}<div class="argument"><strong>{argument.name}:</strong> {argument.description}</div>{/if}
    </div>
  {/if}
</div>

<style>
  .popups {
    position: absolute;
    top: 100%;
    left: 0;
    z-index: 5;
    width: max-content;
    margin-top: 2px;
    font: var(--body);
  }

  .list {
    width: 22rem;
    margin: 0;
    padding: 0.25rem;
    list-style: none;
    border-radius: var(--radius-small);
  }

  .list button {
    display: grid;
    grid-template-columns: auto 1fr;
    gap: 0 0.5rem;
    width: 100%;
    padding: 0.25rem 0.5rem;
    border: 0;
    border-radius: 0.25rem;
    background: transparent;
    color: var(--label);
    text-align: left;
  }

  .list button span {
    grid-column: 1 / -1;
    color: var(--label-secondary);
    font: var(--subheadline);
  }

  .list em {
    color: var(--label-secondary);
    font: var(--footnote);
    font-style: normal;
    align-self: center;
  }

  .list button.highlighted {
    background: var(--accent);
    color: var(--on-accent);
  }

  .list button.highlighted span,
  .list button.highlighted em {
    color: var(--on-accent);
  }

  .footnote {
    padding: 0.25rem 0.5rem 0.125rem;
    color: var(--label-secondary);
    font: var(--footnote);
  }

  .hint {
    min-width: 14rem;
    max-width: 28rem;
    padding: 0.375rem 0.625rem;
    border-radius: var(--radius-small);
  }

  .signature {
    font-family: ui-monospace, "Cascadia Mono", Consolas, monospace;
  }

  .current {
    font-weight: 700;
  }

  .argument {
    margin-top: 0.25rem;
    color: var(--label-secondary);
    font: var(--subheadline);
  }
</style>

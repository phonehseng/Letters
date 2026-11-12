<script lang="ts">
  // the formula bar, as in excel and google sheets: the name box on the
  // leading side (the selected cells, or type "B12" or "A1:C9" to go
  // there), the Explain button (fx), and the active cell's contents,
  // where a formula can be typed with the same help as in the cell.
  import type { CellEditor } from "./cellEditor.svelte";
  import FormulaPopups from "./FormulaPopups.svelte";
  import { parseRangeName, rangeName, type Selection } from "./selection";
  import type { Range } from "./sheetsApi";
  import { tools, tooltipFor } from "../tools";

  interface Props {
    editor: CellEditor;
    selection: Selection;
    /** what the active cell holds, as it would be typed */
    contents: string;
    onGoTo: (range: Range) => void;
    onStartEditing: () => void;
    onKeyDown: (event: KeyboardEvent) => void;
    onExplain: () => void;
    onNameBoxError: (message: string) => void;
  }

  let { editor, selection, contents, onGoTo, onStartEditing, onKeyDown, onExplain, onNameBoxError }: Props = $props();
  let field = $state<HTMLInputElement>();
  let nameBox = $state("");
  let typingName = $state(false);

  const shownName = $derived(selection.ranges.length === 1 && selection.ranges[0].start.row === selection.ranges[0].end.row && selection.ranges[0].start.col === selection.ranges[0].end.col ? rangeName(selection.ranges[0]) : `${rangeName(selection.ranges[selection.ranges.length - 1])}`);
  const typingHere = $derived(editor.active && editor.place === "bar");
  const value = $derived(editor.active ? editor.text : contents);

  $effect(() => {
    if (!typingName) nameBox = shownName;
  });

  // autocomplete, pointing and F4 change the text from outside
  $effect(() => {
    const caret = editor.caret;
    if (field && typingHere && document.activeElement === field && field.selectionStart !== caret) field.setSelectionRange(caret, caret);
  });

  function goToName(event: KeyboardEvent) {
    if (event.key === "Escape") {
      typingName = false;
      nameBox = shownName;
      return;
    }
    if (event.key !== "Enter") return;
    event.preventDefault();
    const range = parseRangeName(nameBox);
    typingName = false;
    if (range) onGoTo(range);
    else onNameBoxError(`“${nameBox}” isn’t a cell or a range. Type something like B12 or A1:C9.`);
  }

  function onFocus() {
    if (!editor.active) onStartEditing();
    else editor.place = "bar";
  }

  function onInput() {
    if (!field) return;
    if (editor.pointing) editor.stopPointing();
    editor.update(field.value, field.selectionStart ?? field.value.length);
  }
</script>

<div class="formula-bar glass">
  <input
    class="name-box"
    aria-label="Name box: the selected cells. Type a cell or a range and press Enter to go there."
    title="The selected cells. Type a cell such as B12, or a range such as A1:C9, and press Enter to go there."
    bind:value={nameBox}
    onfocus={() => (typingName = true)}
    onblur={() => (typingName = false)}
    onkeydown={goToName}
  />
  <button class="fx" title={tooltipFor("sheetsExplainFormula")} aria-label={tools.sheetsExplainFormula.label} onclick={onExplain}>fx</button>
  <div class="contents">
    <input
      bind:this={field}
      {value}
      spellcheck="false"
      aria-label="Contents of the active cell"
      onfocus={onFocus}
      oninput={onInput}
      onkeyup={() => field && typingHere && (editor.caret = field.selectionStart ?? editor.caret)}
      onclick={() => field && typingHere && (editor.caret = field.selectionStart ?? editor.caret)}
      onkeydown={onKeyDown}
    />
    {#if typingHere}<FormulaPopups {editor} />{/if}
  </div>
</div>

<style>
  .formula-bar {
    display: flex;
    align-items: center;
    gap: 0.375rem;
    margin: 0 0.5rem 0.375rem;
    padding: 0.25rem;
    border-radius: var(--radius-group);
  }

  .name-box {
    width: 6.5rem;
    height: 1.75rem;
    padding: 0 0.5rem;
    border: 0;
    border-radius: var(--radius-control);
    background: var(--control);
    color: var(--label);
    font: var(--body);
  }

  .fx {
    height: 1.75rem;
    padding: 0 0.5rem;
    border: 0;
    border-radius: var(--radius-control);
    background: transparent;
    color: var(--label-secondary);
    font: italic 600 0.8125rem/1 Georgia, "Times New Roman", serif;
  }

  .fx:hover {
    background: var(--control-hover);
  }

  .contents {
    position: relative;
    flex: 1;
    min-width: 0;
  }

  .contents input {
    width: 100%;
    height: 1.75rem;
    padding: 0 0.5rem;
    border: 0;
    border-radius: var(--radius-control);
    background: var(--control);
    color: var(--label);
    font: 400 0.8125rem/1 ui-monospace, "Cascadia Mono", Consolas, monospace;
  }

  .contents input:focus,
  .name-box:focus {
    outline: 2px solid var(--accent);
    outline-offset: -1px;
  }
</style>

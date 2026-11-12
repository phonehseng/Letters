<script lang="ts">
  // the text field laid over the cell being typed into. it starts at the
  // cell's size and grows to the right while the text needs room, as
  // excel's does; the formula bar shows the same text (CellEditor).
  import { onMount, tick } from "svelte";
  import type { CellEditor } from "./cellEditor.svelte";
  import FormulaPopups from "./FormulaPopups.svelte";

  interface Props {
    editor: CellEditor;
    /** the cell, in the scroll area's coordinates */
    rect: { x: number; y: number; width: number; height: number };
    zoom: number;
    /** how far the field may grow before it wraps */
    roomRight: number;
    onKeyDown: (event: KeyboardEvent) => void;
  }

  let { editor, rect, zoom, roomRight, onKeyDown }: Props = $props();
  let field = $state<HTMLTextAreaElement>();
  let measurer = $state<HTMLSpanElement>();
  let textWidth = $state(0);

  const width = $derived(Math.min(Math.max(rect.width + 1, textWidth + 12), Math.max(rect.width + 1, roomRight - 4)));

  // the editor's text can change from outside (autocomplete, pointing, F4)
  $effect(() => {
    const text = editor.text;
    const caret = editor.caret;
    if (!field) return;
    if (field.value !== text) field.value = text;
    if (document.activeElement === field && (field.selectionStart !== caret || field.selectionEnd !== caret)) field.setSelectionRange(caret, caret);
    if (measurer) {
      measurer.textContent = text.split("\n").reduce((longest, line) => (line.length > longest.length ? line : longest), "");
      textWidth = measurer.offsetWidth;
    }
  });

  function onInput() {
    if (!field) return;
    if (editor.pointing) editor.stopPointing();
    editor.update(field.value, field.selectionStart);
  }

  function onSelect() {
    if (field && field.selectionStart !== editor.caret) {
      editor.caret = field.selectionStart;
    }
  }

  onMount(async () => {
    await tick();
    field?.focus();
    field?.setSelectionRange(editor.caret, editor.caret);
  });
</script>

<div class="cell-editor" style:left="{rect.x}px" style:top="{rect.y}px" style:width="{width}px" style:min-height="{rect.height}px" style:font-size="{14.67 * zoom}px">
  <span class="measure" bind:this={measurer} aria-hidden="true"></span>
  <textarea
    bind:this={field}
    rows="1"
    spellcheck="false"
    aria-label="Cell contents"
    oninput={onInput}
    onkeyup={onSelect}
    onclick={onSelect}
    onkeydown={onKeyDown}
  ></textarea>
  <FormulaPopups {editor} />
</div>

<style>
  .cell-editor {
    position: absolute;
    z-index: 3;
    display: flex;
    box-shadow:
      0 0 0 2px var(--accent),
      0 4px 12px rgb(0 0 0 / 0.15);
  }

  textarea {
    flex: 1;
    min-height: inherit;
    padding: 1px 3px;
    border: 0;
    outline: 0;
    resize: none;
    overflow: hidden;
    background: #ffffff;
    color: #000000;
    font-family: Calibri, Carlito, "Segoe UI", Arial, sans-serif;
    font-size: inherit;
    line-height: 1.25;
    white-space: pre;
  }

  .measure {
    position: absolute;
    visibility: hidden;
    font-family: Calibri, Carlito, "Segoe UI", Arial, sans-serif;
    white-space: pre;
  }
</style>

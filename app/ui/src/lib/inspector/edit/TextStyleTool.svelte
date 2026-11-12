<script lang="ts">
  // the size, colour and font of the text being edited. letters selected
  // in the editor (within one line) change alone; with nothing selected
  // the whole paragraph changes. the engine can switch existing text only
  // to a standard font, so the font menu offers those (see FontMenu).
  import { INK_SWATCHES } from "../../annotate/palette";
  import Swatches from "../../annotate/Swatches.svelte";
  import { leadingRun } from "../../edit/blocks";
  import type { Rgb, StandardFont } from "../../edit/editApi";
  import { rangeInOneLine } from "../../edit/editSession.svelte";
  import type { ToolContext } from "../../inspector";
  import ToolSection from "../ToolSection.svelte";
  import FontMenu from "./FontMenu.svelte";

  interface Props {
    context: ToolContext;
    open: boolean;
    onToggle: () => void;
  }

  let { context, open, onToggle }: Props = $props();

  const session = $derived(context.edit!);
  const block = $derived(session.editor?.block ?? null);
  const run = $derived(block ? leadingRun(block) : null);
  const partial = $derived(block && session.editor ? rangeInOneLine(block, session.editor.selection) !== null : false);
  const what = $derived(partial ? "the selected letters" : "the whole paragraph");

  function setSize(value: number) {
    if (Number.isFinite(value) && value >= 1 && value <= 400) session.restyle({ size: value });
  }

  function setColor(color: Rgb | null) {
    if (color) session.restyle({ color });
  }

  function setFont(font: StandardFont) {
    session.restyle({ font });
  }
</script>

<ToolSection
  tool="editTextStyle"
  {open}
  {onToggle}
  effect={block
    ? `Changes ${what} on page ${(session.editor?.page ?? 0) + 1}. Text after it on the line moves to make room. Undo with Ctrl Z.`
    : "Click into a paragraph on the page with Edit Text first."}
>
  {#if block && run}
    <label class="setting">
      <span class="caption">Size</span>
      <input
        class="field"
        type="number"
        min="1"
        max="400"
        step="0.5"
        value={Math.round(run.size * 10) / 10}
        onchange={(event) => setSize(Number(event.currentTarget.value))}
      />
    </label>
    <Swatches label="Colour" swatches={INK_SWATCHES} value={run.color} onChange={setColor} />
    <FontMenu label="Font" value={session.currentStandardFont()} onChange={setFont} />
    <p class="hint">Changing the font uses a standard font every PDF reader has; the text’s own font stays for the rest of the page.</p>
  {/if}
</ToolSection>

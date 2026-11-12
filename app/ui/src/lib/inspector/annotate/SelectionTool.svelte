<script lang="ts">
  // the comment or mark clicked on the page: what it is, its style
  // controls (changing one restyles it at once, as one undo step), and
  // Delete. only the controls that fit its kind are shown.
  import type { Rgb } from "../../annotate/annotateApi";
  import { fillOf, kindLabel, strokeWidthOf, styleControls } from "../../annotate/annotations";
  import { commentByline } from "../../annotate/comments";
  import { FILL_SWATCHES, HIGHLIGHT_SWATCHES, INK_SWATCHES, NOTE_SWATCHES } from "../../annotate/palette";
  import Swatches from "../../annotate/Swatches.svelte";
  import type { ToolContext } from "../../inspector";
  import ToolSection from "../ToolSection.svelte";

  interface Props {
    context: ToolContext;
    open: boolean;
    onToggle: () => void;
  }

  let { context, open, onToggle }: Props = $props();

  const session = $derived(context.annotate);
  const selected = $derived(session.selected);
  const controls = $derived(selected ? styleControls(selected) : null);
  const palette = $derived(
    controls?.color === "highlight" ? HIGHLIGHT_SWATCHES : controls?.color === "note" ? NOTE_SWATCHES : INK_SWATCHES,
  );
  const width = $derived(selected ? strokeWidthOf(selected) : null);
  const opacityPercent = $derived(selected ? Math.round(selected.opacity * 100) : 100);
  /** a text box's colour is its text's colour, which the engine keeps in its details */
  const color = $derived(selected?.details.type === "textBox" ? selected.details.textColor : (selected?.color ?? null));

  function setColor(next: Rgb | null) {
    if (next) session.restyle({ color: next });
  }

  const effectLine = $derived.by(() => {
    if (!selected) return "Click a comment or mark on the page to change or delete it.";
    if (!selected.editable) return "Letters can’t change this kind of annotation, but Delete removes it.";
    if (selected.details.type === "textMarkup") return "Change its colour here, or press Delete. It stays on its words.";
    return "Drag it on the page, or use its handles. The arrow keys move it 1 point, 10 with Shift.";
  });
</script>

<ToolSection tool="editAnnotation" {open} {onToggle} effect={effectLine}>
  {#if selected && controls}
    <p class="what">
      <strong>{kindLabel(selected)}</strong>
      <span class="hint">{commentByline(selected, Date.now() / 1000)}</span>
    </p>
    {#if controls.color !== "none"}
      <Swatches label="Colour" swatches={palette} value={color} onChange={setColor} />
    {/if}
    {#if controls.strokeWidth && width !== null}
      <label class="setting">
        <span class="caption">Line Width: {width} pt</span>
        <input
          type="range"
          min="0.5"
          max="14"
          step="0.5"
          value={width}
          aria-valuetext={`${width} points`}
          onchange={(event) => session.restyle({ strokeWidth: Number(event.currentTarget.value) })}
        />
      </label>
    {/if}
    {#if controls.fill}
      <Swatches label="Fill" swatches={FILL_SWATCHES} allowNone value={fillOf(selected)} onChange={(fill) => session.restyle({ fill })} />
    {/if}
    {#if controls.opacity}
      <label class="setting">
        <span class="caption">Opacity: {opacityPercent}%</span>
        <input
          type="range"
          min="10"
          max="100"
          step="5"
          value={opacityPercent}
          aria-valuetext={`${opacityPercent}%`}
          onchange={(event) => session.restyle({ opacity: Number(event.currentTarget.value) / 100 })}
        />
      </label>
    {/if}
    <div class="buttons">
      {#if controls.text}
        <button class="button" onclick={() => session.editTextBox(selected)}>Edit Text</button>
      {/if}
      {#if selected.kind === "note"}
        <button class="button" onclick={() => session.openNote(selected)}>Open Note</button>
      {/if}
      <button class="button" title="Delete it, with any replies (Delete)" onclick={() => session.deleteSelected()}>Delete</button>
    </div>
  {/if}
</ToolSection>

<style>
  .what {
    display: flex;
    flex-direction: column;
    gap: 0.125rem;
    margin: 0;
  }

  .buttons {
    display: flex;
    flex-wrap: wrap;
    justify-content: flex-end;
    gap: 0.5rem;
  }
</style>

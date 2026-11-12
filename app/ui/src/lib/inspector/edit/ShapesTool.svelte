<script lang="ts">
  // the selected shapes: their fill, outline, line width, dash and
  // opacity, turning, restacking, duplicating and deleting. moving and
  // resizing are done on the page with the frame, and a rectangle's
  // corners or a line's ends with their round handles. each control makes
  // its change when it is set, as one undo step.
  import type { NewPaint } from "../../edit/contentApi";
  import { hexToRgb, rgbToHex } from "../../edit/cssFonts";
  import type { ToolContext } from "../../inspector";
  import ToolSection from "../ToolSection.svelte";

  interface Props {
    context: ToolContext;
    open: boolean;
    onToggle: () => void;
  }

  let { context, open, onToggle }: Props = $props();

  const BLACK = { r: 0, g: 0, b: 0 };

  const session = $derived(context.edit!);
  const shapes = $derived(session.shapes);
  const first = $derived(shapes.shapes[0] ?? null);

  // dash patterns in points, as seen; the empty one is a solid line
  const DASHES: { id: string; label: string; dash: number[] }[] = [
    { id: "solid", label: "Solid", dash: [] },
    { id: "dashed", label: "Dashed", dash: [6, 3] },
    { id: "dotted", label: "Dotted", dash: [1, 2] },
  ];

  const dashId = $derived(DASHES.find((each) => each.dash.join(" ") === (first?.stroke?.dash ?? []).join(" "))?.id ?? "solid");
  const opacity = $derived(Math.round(Math.max(first?.fillOpacity ?? 1, first?.strokeOpacity ?? 1) * 100));

  function described(): string {
    if (!first || !shapes.selected) return "";
    const count = shapes.shapes.length;
    const kind = count > 1 ? `${count} shapes drawn as one` : first.shape === "free" ? "a drawing" : `a ${first.shape}`;
    return `Page ${shapes.selected.page + 1}: ${kind}${first.clipped ? ", partly hidden by a clip" : ""}.`;
  }

  function paint(hex: string): NewPaint {
    return { type: "rgb", color: hexToRgb(hex) };
  }
</script>

<ToolSection tool="pageShapes" {open} {onToggle} effect={first ? "Each change is one Undo. Drag the shape to move it; drag its handles to resize it." : "Click a shape on the page to select it. A logo drawn from many shapes is selected as one."}>
  {#if first}
    <p class="hint">{described()}</p>
    <div class="grid">
      <label class="field">
        <span class="caption">Fill</span>
        <input type="color" value={rgbToHex(first.fill?.color ?? BLACK)} disabled={!first.fill} onchange={(event) => shapes.restyle({ fill: paint(event.currentTarget.value) })} />
      </label>
      <label class="check">
        <input type="checkbox" checked={first.fill !== null} onchange={(event) => shapes.restyle({ fill: event.currentTarget.checked ? paint("#808080") : { type: "none" } })} />
        <span>Filled</span>
      </label>
      <label class="field">
        <span class="caption">Outline</span>
        <input type="color" value={rgbToHex(first.stroke?.paint.color ?? BLACK)} disabled={!first.stroke} onchange={(event) => shapes.restyle({ stroke: paint(event.currentTarget.value) })} />
      </label>
      <label class="check">
        <input type="checkbox" checked={first.stroke !== null} onchange={(event) => shapes.restyle({ stroke: event.currentTarget.checked ? paint("#000000") : { type: "none" } })} />
        <span>Outlined</span>
      </label>
      <label class="field">
        <span class="caption">Line width (pt)</span>
        <input type="number" min="0" step="0.25" value={first.stroke?.width ?? 0} disabled={!first.stroke} onchange={(event) => shapes.restyle({ strokeWidth: Number(event.currentTarget.value) })} />
      </label>
      <label class="field">
        <span class="caption">Line</span>
        <select value={dashId} disabled={!first.stroke} onchange={(event) => shapes.restyle({ dash: DASHES.find((each) => each.id === event.currentTarget.value)?.dash ?? [] })}>
          {#each DASHES as each (each.id)}<option value={each.id}>{each.label}</option>{/each}
        </select>
      </label>
    </div>
    <label class="field">
      <span class="caption">Opacity: {opacity}%</span>
      <input type="range" min="0" max="100" value={opacity} onchange={(event) => shapes.restyle({ opacity: Number(event.currentTarget.value) / 100 })} />
    </label>
    {#if shapes.pointShape}
      <p class="hint">Drag the round handles to move its {shapes.pointShape.shape === "line" ? "ends" : "corners"}; its line width stays the same.</p>
    {/if}
    <div class="row">
      <button class="button" onclick={() => shapes.rotate(-90)}>Turn Left</button>
      <button class="button" onclick={() => shapes.rotate(90)}>Turn Right</button>
    </div>
    <div class="row">
      <button class="button" onclick={() => shapes.restack(true)}>Bring to Front</button>
      <button class="button" onclick={() => shapes.restack(false)}>Send to Back</button>
    </div>
    <div class="row">
      <button class="button" onclick={() => shapes.duplicate()}>Duplicate</button>
      <button class="button" onclick={() => shapes.delete()}>Delete</button>
    </div>
  {:else if session.tool !== "edit"}
    <button class="button" onclick={() => session.setTool("edit")}>Edit Text and Images</button>
  {/if}
</ToolSection>

<style>
  .grid {
    display: grid;
    grid-template-columns: repeat(2, minmax(0, 1fr));
    align-items: end;
    gap: 0.5rem;
  }

  .field {
    display: flex;
    flex-direction: column;
    gap: 0.125rem;
  }

  .row {
    display: flex;
    flex-wrap: wrap;
    gap: 0.375rem;
  }
</style>

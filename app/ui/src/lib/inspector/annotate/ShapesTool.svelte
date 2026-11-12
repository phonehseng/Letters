<script lang="ts">
  // rectangles, ellipses, lines and arrows, drawn by dragging on the
  // page. Shift makes a square or a circle, or keeps a line straight.
  import { untrack } from "svelte";
  import { placeEffect } from "../../annotate/annotateEffects";
  import type { ShapeChoice } from "../../annotate/annotateSession.svelte";
  import { FILL_SWATCHES, INK_SWATCHES } from "../../annotate/palette";
  import Swatches from "../../annotate/Swatches.svelte";
  import type { ToolContext } from "../../inspector";
  import Choices from "../Choices.svelte";
  import ToolSection from "../ToolSection.svelte";

  interface Props {
    context: ToolContext;
    open: boolean;
    onToggle: () => void;
  }

  let { context, open, onToggle }: Props = $props();

  const NAMES: Record<ShapeChoice, string> = { square: "Rectangle", circle: "Ellipse", line: "Line", arrow: "Arrow" };

  const session = $derived(context.annotate);
  const shape = $derived(session.styles.shape);
  const active = $derived(session.tool === "shape");
  const filled = $derived(shape.kind === "square" || shape.kind === "circle");
  let kind = $state<ShapeChoice>(untrack(() => context.annotate.styles.shape.kind));

  $effect(() => {
    const chosen = kind;
    untrack(() => (session.styles.shape.kind = chosen));
  });

  // closing the section puts its tool away. only a close does: a section
  // that starts closed (the tab just opened) leaves a tool picked by a shortcut alone
  let wasOpen = false;
  $effect(() => {
    const isOpen = open;
    untrack(() => {
      if (wasOpen && !isOpen && session.tool === "shape") session.setTool("select");
    });
    wasOpen = isOpen;
  });

  function act(event: MouseEvent) {
    if (event.detail === 0) return session.placeFromKeyboard("shape");
    session.setTool(active ? "select" : "shape");
  }

  const how = $derived(
    filled
      ? `Drag on the page to draw ${shape.kind === "square" ? "a rectangle" : "an ellipse"}; hold Shift for ${shape.kind === "square" ? "a square" : "a circle"}.`
      : "Drag on the page to draw it; hold Shift to keep it straight or at 45 degrees.",
  );
</script>

<ToolSection
  tool="drawShapes"
  {open}
  {onToggle}
  actionLabel={active ? "Stop Drawing" : `Draw ${NAMES[shape.kind]}`}
  pressed={active}
  effect={placeEffect(`the ${NAMES[shape.kind].toLowerCase()}`, how, active)}
  onAction={act}
>
  <Choices
    label="Shape"
    options={[
      { value: "square", label: "Rectangle" },
      { value: "circle", label: "Ellipse" },
      { value: "line", label: "Line" },
      { value: "arrow", label: "Arrow" },
    ]}
    bind:value={kind}
  />
  <Swatches label="Colour" swatches={INK_SWATCHES} value={shape.color} onChange={(color) => color && (session.styles.shape.color = color)} />
  <label class="setting">
    <span class="caption">Line Width: {shape.width} pt</span>
    <input
      type="range"
      min="0.5"
      max="12"
      step="0.5"
      value={shape.width}
      aria-valuetext={`${shape.width} points`}
      oninput={(event) => (session.styles.shape.width = Number(event.currentTarget.value))}
    />
  </label>
  {#if filled}
    <Swatches label="Fill" swatches={FILL_SWATCHES} allowNone value={shape.fill} onChange={(color) => (session.styles.shape.fill = color)} />
  {/if}
  <label class="setting">
    <span class="caption">Opacity: {Math.round(shape.opacity * 100)}%</span>
    <input
      type="range"
      min="10"
      max="100"
      step="5"
      value={Math.round(shape.opacity * 100)}
      aria-valuetext={`${Math.round(shape.opacity * 100)}%`}
      oninput={(event) => (session.styles.shape.opacity = Number(event.currentTarget.value) / 100)}
    />
  </label>
</ToolSection>

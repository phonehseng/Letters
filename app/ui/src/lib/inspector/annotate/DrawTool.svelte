<script lang="ts">
  // freehand drawing with a pen or a highlighter. strokes made without a
  // pause become one drawing, saved a moment after the last stroke, so
  // Undo takes back a whole word or sketch.
  import { untrack } from "svelte";
  import { HIGHLIGHTER, INK_SWATCHES } from "../../annotate/palette";
  import Swatches from "../../annotate/Swatches.svelte";
  import type { ToolContext } from "../../inspector";
  import { UNDO_HINT } from "../../pageEffects";
  import Choices from "../Choices.svelte";
  import ToolSection from "../ToolSection.svelte";

  interface Props {
    context: ToolContext;
    open: boolean;
    onToggle: () => void;
  }

  let { context, open, onToggle }: Props = $props();

  const session = $derived(context.annotate);
  const ink = $derived(session.styles.ink);
  const active = $derived(session.tool === "draw");
  let pen = $state<"pen" | "highlighter">(untrack(() => (context.annotate.styles.ink.highlighter ? "highlighter" : "pen")));

  $effect(() => {
    const chosen = pen;
    untrack(() => (session.styles.ink.highlighter = chosen === "highlighter"));
  });

  // closing the section puts its tool away. only a close does: a section
  // that starts closed (the tab just opened) leaves a tool picked by a shortcut alone
  let wasOpen = false;
  $effect(() => {
    const isOpen = open;
    untrack(() => {
      if (wasOpen && !isOpen && session.tool === "draw") session.setTool("select");
    });
    wasOpen = isOpen;
  });

  const effectLine = $derived(
    active
      ? "Draw on the page. Strokes made without a pause become one drawing. Press Escape when you are done."
      : `Adds your drawing to your working copy. ${UNDO_HINT} Drawing needs a mouse, a pen or a finger.`,
  );
</script>

<ToolSection
  tool="drawInk"
  {open}
  {onToggle}
  actionLabel={active ? "Stop Drawing" : "Start Drawing"}
  pressed={active}
  effect={effectLine}
  onAction={() => session.setTool(active ? "select" : "draw")}
>
  <Choices
    label="Pen"
    options={[
      { value: "pen", label: "Pen" },
      { value: "highlighter", label: "Highlighter", title: "Wide and see-through" },
    ]}
    bind:value={pen}
  />
  {#if pen === "pen"}
    <Swatches label="Colour" swatches={INK_SWATCHES} value={ink.color} onChange={(color) => color && (session.styles.ink.color = color)} />
    <label class="setting">
      <span class="caption">Width: {ink.width} pt</span>
      <input
        type="range"
        min="0.5"
        max="12"
        step="0.5"
        value={ink.width}
        aria-valuetext={`${ink.width} points`}
        oninput={(event) => (session.styles.ink.width = Number(event.currentTarget.value))}
      />
    </label>
  {:else}
    <p class="hint">A yellow highlighter {HIGHLIGHTER.width} points wide, see-through so the words show.</p>
  {/if}
</ToolSection>

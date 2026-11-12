<script lang="ts">
  // a text box typed straight onto the page. the font, size, colour,
  // alignment and background here are what the box is typed in, and
  // changing them while typing changes the box being typed.
  import { untrack } from "svelte";
  import { FONT_FAMILIES, placeEffect, type FontFamily } from "../../annotate/annotateEffects";
  import type { TextAlign } from "../../annotate/annotateApi";
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

  const session = $derived(context.annotate);
  const style = $derived(session.styles.textBox);
  const active = $derived(session.tool === "textBox");
  const typing = $derived(session.editing !== null);

  let family = $state<FontFamily>(untrack(() => context.annotate.styles.textBox.family));
  let align = $state<TextAlign>(untrack(() => context.annotate.styles.textBox.align));

  // the choices follow a box opened for editing, and change it when changed
  $effect(() => {
    family = style.family;
    align = style.align;
  });
  $effect(() => {
    const [chosenFamily, chosenAlign] = [family, align];
    untrack(() => {
      if (style.family !== chosenFamily) session.styles.textBox.family = chosenFamily;
      if (style.align !== chosenAlign) session.styles.textBox.align = chosenAlign;
    });
  });

  // closing the section puts its tool away. only a close does: a section
  // that starts closed (the tab just opened) leaves a tool picked by a shortcut alone
  let wasOpen = false;
  $effect(() => {
    const isOpen = open;
    untrack(() => {
      if (wasOpen && !isOpen && session.tool === "textBox") session.setTool("select");
    });
    wasOpen = isOpen;
  });

  function act(event: MouseEvent) {
    if (event.detail === 0) return session.placeFromKeyboard("textBox");
    session.setTool(active ? "select" : "textBox");
  }

  function setSize(value: number) {
    if (Number.isFinite(value)) session.styles.textBox.size = Math.min(Math.max(value, 4), 144);
  }
</script>

<ToolSection
  tool="addTextBox"
  {open}
  {onToggle}
  actionLabel={active ? "Cancel" : "Add Text Box"}
  pressed={active}
  effect={typing
    ? "Type on the page. Click outside the box or press Ctrl Enter to finish; Escape cancels."
    : placeEffect("a text box", "Click the page for a box, or drag to choose its width.", active)}
  onAction={act}
>
  <Choices label="Font" options={FONT_FAMILIES} bind:value={family} />
  <div class="setting-row">
    <label class="setting">
      <span class="caption">Size</span>
      <input class="field" type="number" min="4" max="144" step="1" value={style.size} onchange={(event) => setSize(Number(event.currentTarget.value))} />
    </label>
    <label class="check">
      <input type="checkbox" checked={style.bold} onchange={(event) => (session.styles.textBox.bold = event.currentTarget.checked)} />
      <span>Bold</span>
    </label>
    <label class="check">
      <input type="checkbox" checked={style.italic} onchange={(event) => (session.styles.textBox.italic = event.currentTarget.checked)} />
      <span>Italic</span>
    </label>
  </div>
  <Choices
    label="Alignment"
    options={[
      { value: "left", label: "Left" },
      { value: "center", label: "Centre" },
      { value: "right", label: "Right" },
    ]}
    bind:value={align}
  />
  <Swatches label="Text Colour" swatches={INK_SWATCHES} value={style.color} onChange={(color) => color && (session.styles.textBox.color = color)} />
  <Swatches label="Background" swatches={FILL_SWATCHES} allowNone value={style.fill} onChange={(color) => (session.styles.textBox.fill = color)} />
  <p class="hint">Double-click a text box on the page to change its words.</p>
</ToolSection>

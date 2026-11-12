<script lang="ts">
  // a signature typed in a handwriting style. the styles are fonts this
  // computer already has (nothing is downloaded); each choice shows the
  // name in that font. Keep Signature draws it large on a see-through
  // canvas and keeps that png.
  import { untrack } from "svelte";
  import { canvasPng } from "./canvasPng";
  import { cleanSignatureText, TYPED_FONT_PX, TYPED_STYLES, typedCanvasSize, type TypedStyleId } from "./signaturePicture";

  interface Props {
    /** the name to start with, such as the name on comments */
    initialName: string;
    onKeep: (png: Uint8Array) => Promise<unknown>;
  }

  let { initialName, onKeep }: Props = $props();

  const INK = "#1b1f3b";
  let name = $state(untrack(() => initialName));
  let style = $state<TypedStyleId>("script");
  let busy = $state(false);
  const text = $derived(cleanSignatureText(name));
  const family = $derived(TYPED_STYLES.find((each) => each.id === style)!.family);

  async function keep() {
    if (!text) return;
    busy = true;
    try {
      const canvas = document.createElement("canvas");
      const context = canvas.getContext("2d")!;
      const font = `${TYPED_FONT_PX}px ${family}`;
      context.font = font;
      const size = typedCanvasSize(context.measureText(text).width);
      canvas.width = size.width;
      canvas.height = size.height;
      // changing the size resets the drawing state, so the font is set again
      context.font = font;
      context.fillStyle = INK;
      context.fillText(text, size.x, size.baseline);
      await onKeep(await canvasPng(canvas));
    } finally {
      busy = false;
    }
  }
</script>

<label class="setting">
  <span class="caption">Your Name</span>
  <input class="field" bind:value={name} maxlength="60" autocomplete="name" />
</label>
<fieldset class="styles">
  <legend class="caption">Style</legend>
  {#each TYPED_STYLES as option (option.id)}
    <label class="style" class:chosen={style === option.id}>
      <input type="radio" name="typed-style" value={option.id} bind:group={style} />
      <span class="sample" style:font-family={option.family}>{text || "Your Name"}</span>
      <span class="visually-hidden">{option.label}</span>
    </label>
  {/each}
</fieldset>
<div class="buttons">
  <button class="button" disabled={!text || busy} onclick={keep}>Keep Signature</button>
</div>

<style>
  .styles {
    display: flex;
    flex-direction: column;
    gap: 0.25rem;
    margin: 0;
    padding: 0;
    border: 0;
  }

  .style {
    position: relative;
    display: block;
    padding: 0.25rem 0.5rem;
    border-radius: var(--radius-small);
    background: #ffffff;
    box-shadow: inset 0 0 0 0.5px var(--separator);
    color: #1b1f3b;
  }

  .style.chosen {
    box-shadow: inset 0 0 0 1.5px var(--accent);
  }

  .style input {
    position: absolute;
    opacity: 0;
  }

  .style:focus-within {
    outline: 3px solid var(--accent-soft);
  }

  .sample {
    display: block;
    overflow: hidden;
    font-size: 1.375rem;
    line-height: 2rem;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .buttons {
    display: flex;
    justify-content: flex-end;
  }
</style>

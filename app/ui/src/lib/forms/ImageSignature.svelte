<script lang="ts">
  // a signature from a picture file: a photo or scan of a signature on
  // paper. "Make white see-through" takes the paper away so only the ink
  // shows on the page. the picture is shrunk to a sensible size first.
  import { asCommandError } from "../api";
  import { choosePicture, fileNameOf } from "../pageApi";
  import { canvasPng, loadImage } from "./canvasPng";
  import { readPicture } from "./formsApi";
  import { fitWithin, whiteToTransparent } from "./signaturePicture";

  interface Props {
    onKeep: (png: Uint8Array) => Promise<unknown>;
    onError: (message: string) => void;
  }

  let { onKeep, onError }: Props = $props();

  /** big enough to print sharply at signature size, small enough to keep */
  const MAX_SIZE = { width: 1200, height: 600 };

  let image = $state<HTMLImageElement | null>(null);
  let fileName = $state("");
  let clearWhite = $state(true);
  let preview = $state("");
  let busy = $state(false);

  async function choose() {
    const path = await choosePicture();
    if (!path) return;
    try {
      image = await loadImage(await readPicture(path));
      fileName = fileNameOf(path);
      preview = render().toDataURL("image/png");
    } catch (problem) {
      onError(asCommandError(problem).message);
    }
  }

  /** the picture shrunk to size, with the paper see-through when asked. */
  function render(): HTMLCanvasElement {
    const canvas = document.createElement("canvas");
    if (!image) return canvas;
    const size = fitWithin(image.naturalWidth, image.naturalHeight, MAX_SIZE.width, MAX_SIZE.height);
    canvas.width = size.width;
    canvas.height = size.height;
    const context = canvas.getContext("2d")!;
    context.drawImage(image, 0, 0, size.width, size.height);
    if (clearWhite) {
      const pixels = context.getImageData(0, 0, size.width, size.height);
      whiteToTransparent(pixels.data);
      context.putImageData(pixels, 0, 0);
    }
    return canvas;
  }

  // the preview follows the checkbox
  $effect(() => {
    void clearWhite;
    if (image) preview = render().toDataURL("image/png");
  });

  async function keep() {
    if (!image) return;
    busy = true;
    try {
      if (await onKeep(await canvasPng(render()))) {
        image = null;
        preview = "";
      }
    } finally {
      busy = false;
    }
  }
</script>

<div class="setting">
  <button class="button" onclick={choose}>{image ? "Choose Another Picture…" : "Choose Picture…"}</button>
  {#if image}<p class="hint">“{fileName}”</p>{/if}
</div>
{#if image}
  <div class="preview"><img src={preview} alt="Your signature" /></div>
  <label class="check"><input type="checkbox" bind:checked={clearWhite} /> Make white see-through</label>
  <div class="buttons">
    <button class="button" disabled={busy} onclick={keep}>Keep Signature</button>
  </div>
{/if}

<style>
  /* a chequered background shows which parts are see-through */
  .preview {
    display: grid;
    place-items: center;
    padding: 0.5rem;
    border-radius: var(--radius-small);
    background: repeating-conic-gradient(#ececec 0% 25%, #ffffff 0% 50%) 50% / 12px 12px;
  }

  .preview img {
    max-width: 100%;
    max-height: 6rem;
  }

  .buttons {
    display: flex;
    justify-content: flex-end;
  }
</style>

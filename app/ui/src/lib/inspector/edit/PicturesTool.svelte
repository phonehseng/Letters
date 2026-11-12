<script lang="ts">
  // the selected picture: turn it, crop it (on the page, with handles),
  // flip it, change its opacity, replace it (keeping its place, fitted,
  // filled or stretched), bring it to the front or send it to the back,
  // save it as its own file, or delete it. moving and resizing are done
  // on the page with its frame (and the arrows, Alt with the arrows).
  import { save as showSaveDialog } from "@tauri-apps/plugin-dialog";
  import { imageEffect } from "../../edit/editSentences";
  import type { ImageFit } from "../../edit/editApi";
  import type { ToolContext } from "../../inspector";
  import { choosePicture, fileNameOf, pathBeside, stemOf } from "../../pageApi";
  import Choices from "../Choices.svelte";
  import ToolSection from "../ToolSection.svelte";

  interface Props {
    context: ToolContext;
    open: boolean;
    onToggle: () => void;
  }

  let { context, open, onToggle }: Props = $props();

  const session = $derived(context.edit!);
  const image = $derived(session.pictures.image);
  const page = $derived(session.pictures.selected?.page ?? context.currentPage);

  const FITS: { value: ImageFit; label: string }[] = [
    { value: "fit", label: "Fit" },
    { value: "fill", label: "Fill" },
    { value: "stretch", label: "Stretch" },
  ];

  async function replace() {
    const path = await choosePicture();
    if (path && (await session.pictures.replace(path))) context.notify(`Replaced the picture with “${fileNameOf(path)}”.`);
  }

  async function saveAs() {
    if (!image) return;
    const extension = image.filter === "DCTDecode" ? "jpg" : "png";
    const suggested = pathBeside(context.document.path, `${stemOf(context.document.path)} - picture.${extension}`);
    const chosen = await showSaveDialog({
      defaultPath: suggested,
      filters: [{ name: extension === "jpg" ? "JPEG pictures" : "PNG pictures", extensions: [extension] }],
    });
    if (typeof chosen !== "string") return;
    const written = await session.pictures.save(chosen);
    if (written) context.notify(`Saved the picture as “${fileNameOf(written)}”.`);
  }
</script>

<ToolSection tool="pageImages" {open} {onToggle} effect={imageEffect(image !== null)}>
  {#if image}
    <p class="hint">
      Page {page + 1}: {image.pixelWidth} × {image.pixelHeight} pixels{image.inline ? ", written inside the page" : ""}.
    </p>
    <div class="row">
      <button class="button" onclick={() => session.pictures.rotate(-90)}>Turn Left</button>
      <button class="button" onclick={() => session.pictures.rotate(90)}>Turn Right</button>
    </div>
    <div class="row">
      <button class="button" title="Shows only part of the picture; every pixel stays in the file" onclick={() => session.pictures.startCrop()}>Crop</button>
      <button class="button" title="Mirror the picture left to right" onclick={() => session.pictures.flip("horizontal")}>Flip Across</button>
      <button class="button" title="Mirror the picture top to bottom" onclick={() => session.pictures.flip("vertical")}>Flip Down</button>
    </div>
    {#if session.pictures.cropping}
      <p class="hint">Drag the edges or corners of the rectangle on the page, then choose Done. The hidden part stays in the file, so the crop can be undone.</p>
    {/if}
    <label class="opacity">
      <span class="caption">Opacity: {Math.round((image.opacity ?? 1) * 100)}%</span>
      <input type="range" min="0" max="100" value={Math.round((image.opacity ?? 1) * 100)} onchange={(event) => session.pictures.setOpacity(Number(event.currentTarget.value) / 100)} />
    </label>
    <div class="row">
      <button class="button" onclick={() => session.pictures.restack(true)}>Bring to Front</button>
      <button class="button" onclick={() => session.pictures.restack(false)}>Send to Back</button>
    </div>
    <Choices label="Replace with a picture that" options={FITS} bind:value={session.pictures.replaceFit} />
    <p class="hint">
      {session.pictures.replaceFit === "fit"
        ? "Fit keeps the new picture’s proportions inside the old place."
        : session.pictures.replaceFit === "fill"
          ? "Fill covers the old place, keeping proportions; what spills over is cut off."
          : "Stretch makes the new picture exactly the old place, even if that squashes it."}
    </p>
    <div class="row">
      <button class="button" disabled={image.inline} title={image.inline ? "This picture is written inside the page and can’t be replaced" : undefined} onclick={replace}>Replace…</button>
      <button class="button" disabled={image.inline} title={image.inline ? "This picture is written inside the page and can’t be saved on its own" : undefined} onclick={saveAs}>Save Image As…</button>
      <button class="button" onclick={() => session.pictures.delete()}>Delete</button>
    </div>
  {:else if session.tool !== "edit"}
    <button class="button" onclick={() => session.setTool("edit")}>Edit Text and Images</button>
  {/if}
</ToolSection>

<style>
  .row {
    display: flex;
    flex-wrap: wrap;
    gap: 0.375rem;
  }

  .opacity {
    display: flex;
    flex-direction: column;
    gap: 0.125rem;
  }
</style>

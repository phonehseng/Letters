<script lang="ts">
  // Add Image: a png or jpeg put into the page itself. choose a file and
  // click the page (two inches wide) or drag its size; or paste a picture
  // with Ctrl V, or drop a file on the page, while editing.
  import { untrack } from "svelte";
  import type { ToolContext } from "../../inspector";
  import { choosePicture, fileNameOf } from "../../pageApi";
  import ToolSection from "../ToolSection.svelte";

  interface Props {
    context: ToolContext;
    open: boolean;
    onToggle: () => void;
  }

  let { context, open, onToggle }: Props = $props();

  const session = $derived(context.edit!);
  const active = $derived(session.tool === "addImage");
  const chosen = $derived(session.pictures.chosen);

  let wasOpen = false;
  $effect(() => {
    const isOpen = open;
    untrack(() => {
      if (wasOpen && !isOpen && session.tool === "addImage") session.setTool("none");
    });
    wasOpen = isOpen;
  });

  async function choose() {
    const path = await choosePicture();
    if (path) await session.pictures.choose(path);
  }

  function act(event: MouseEvent) {
    if (event.detail === 0) return session.pictures.placeChosenInView();
    session.setTool(active ? "none" : "addImage");
  }
</script>

<ToolSection
  tool="addPageImage"
  {open}
  {onToggle}
  actionLabel={active ? "Stop Placing" : "Place Image"}
  pressed={active}
  ready={chosen !== null}
  effect={chosen
    ? active
      ? `Click the page to place “${fileNameOf(chosen.path)}”, or drag to choose its size. Press Escape to stop.`
      : `Places “${fileNameOf(chosen.path)}” on the page you click. The file on disk changes only when you choose Save.`
    : "Choose a PNG or JPEG picture first, paste one with Ctrl V, or drop one on the page."}
  onAction={act}
>
  <div class="setting">
    <span class="caption">Picture</span>
    <button class="button" onclick={choose}>{chosen ? "Choose Another Picture…" : "Choose Picture…"}</button>
    {#if chosen}<p class="hint">“{fileNameOf(chosen.path)}”. It keeps its proportions when resized.</p>{/if}
  </div>
</ToolSection>

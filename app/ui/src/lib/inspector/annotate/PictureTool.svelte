<script lang="ts">
  // a picture on the page: choose a png or jpeg, then click the page (it
  // is placed two inches wide) or drag to choose its size. it keeps its
  // proportions when placed and when resized.
  import { untrack } from "svelte";
  import { pictureSize } from "../../annotate/annotateApi";
  import { placeEffect } from "../../annotate/annotateEffects";
  import { asCommandError } from "../../api";
  import type { ToolContext } from "../../inspector";
  import { choosePicture, fileNameOf } from "../../pageApi";
  import ToolSection from "../ToolSection.svelte";

  interface Props {
    context: ToolContext;
    open: boolean;
    onToggle: () => void;
  }

  let { context, open, onToggle }: Props = $props();

  const session = $derived(context.annotate);
  const active = $derived(session.tool === "picture");
  const chosen = $derived(session.picture);

  // closing the section puts its tool away. only a close does: a section
  // that starts closed (the tab just opened) leaves a tool picked by a shortcut alone
  let wasOpen = false;
  $effect(() => {
    const isOpen = open;
    untrack(() => {
      if (wasOpen && !isOpen && session.tool === "picture") session.setTool("select");
    });
    wasOpen = isOpen;
  });

  async function choose() {
    const path = await choosePicture();
    if (!path) return;
    try {
      const size = await pictureSize(path);
      session.picture = { path, aspect: size.width / size.height };
      session.setTool("picture");
    } catch (problem) {
      context.showError(asCommandError(problem).message);
    }
  }

  function act(event: MouseEvent) {
    if (event.detail === 0) return session.placeFromKeyboard("picture");
    session.setTool(active ? "select" : "picture");
  }
</script>

<ToolSection
  tool="addPicture"
  {open}
  {onToggle}
  actionLabel={active ? "Stop Placing" : "Place Picture"}
  pressed={active}
  ready={chosen !== null}
  effect={chosen
    ? placeEffect(`“${fileNameOf(chosen.path)}”`, "Click the page to place the picture, or drag to choose its size.", active)
    : "Choose a PNG or JPEG picture first."}
  onAction={act}
>
  <div class="setting">
    <span class="caption">Picture</span>
    <button class="button" onclick={choose}>{chosen ? "Choose Another Picture…" : "Choose Picture…"}</button>
    {#if chosen}<p class="hint">“{fileNameOf(chosen.path)}”. It keeps its proportions when resized.</p>{/if}
  </div>
</ToolSection>

<script lang="ts">
  // Edit Text and Images: the page tool that makes every paragraph,
  // picture and shape of the page a target. with it, and with what keeping
  // the original font can and can't do, said once and honestly.
  import { untrack } from "svelte";
  import { leadingRun } from "../../edit/blocks";
  import { fontPromise, editTextEffect, KEEPING_THE_FONT } from "../../edit/editSentences";
  import type { ToolContext } from "../../inspector";
  import ToolSection from "../ToolSection.svelte";

  interface Props {
    context: ToolContext;
    open: boolean;
    onToggle: () => void;
  }

  let { context, open, onToggle }: Props = $props();

  const session = $derived(context.edit!);
  const active = $derived(session.tool === "edit");
  const run = $derived(session.editor?.block ? leadingRun(session.editor.block) : null);

  // closing the section puts the tool away, unless a paragraph is being typed in
  // or a picture or shape was just selected (which opens its own section)
  let wasOpen = false;
  $effect(() => {
    const isOpen = open;
    untrack(() => {
      const selecting = session.editor || session.pictures.selected || session.shapes.selected;
      if (wasOpen && !isOpen && session.tool === "edit" && !selecting) session.setTool("none");
    });
    wasOpen = isOpen;
  });

  function act() {
    session.setTool(active ? "none" : "edit");
    // from the keyboard, Tab then reaches the paragraphs on the page
    if (!active) context.notify("Click a paragraph to type in it, or a picture to select it. Tab moves through them from the keyboard.");
  }
</script>

<ToolSection
  tool="editText"
  {open}
  {onToggle}
  actionLabel={active ? "Stop Editing" : "Edit Text"}
  pressed={active}
  effect={editTextEffect(active)}
  onAction={act}
>
  <p class="hint">{fontPromise(run?.font ?? null)}</p>
  <details class="limits">
    <summary>What keeping the font can and can’t do</summary>
    {#each KEEPING_THE_FONT as sentence (sentence)}
      <p class="hint">{sentence}</p>
    {/each}
  </details>
</ToolSection>

<style>
  .limits summary {
    cursor: pointer;
    color: var(--label-secondary);
  }

  .limits p {
    margin: 0.375rem 0 0;
  }
</style>

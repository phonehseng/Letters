<script lang="ts">
  // Add Field: choose the kind (and, for radio buttons and pop-up menus,
  // the choices), then drag on the page, or click for the usual size. the
  // button pressed from the keyboard puts the field in the middle of the
  // view. the new field is selected in Edit Fields, where its name, help
  // text and other settings can be changed.
  import { untrack } from "svelte";
  import { addFieldEffect } from "../../forms/fillSignEffects";
  import { ADDABLE_KINDS, needsChoices, type AddableKind } from "../../forms/newFields";
  import type { ToolContext } from "../../inspector";
  import ToolSection from "../ToolSection.svelte";

  interface Props {
    context: ToolContext;
    open: boolean;
    onToggle: () => void;
  }

  let { context, open, onToggle }: Props = $props();

  const session = $derived(context.fillSign!);
  const drawing = $derived(session.tool === "drawField");
  const label = $derived(ADDABLE_KINDS.find((each) => each.kind === session.newKind)?.label ?? "Text");

  // closing the section puts its tool away
  let wasOpen = false;
  $effect(() => {
    const isOpen = open;
    untrack(() => {
      if (wasOpen && !isOpen && session.tool === "drawField") session.setTool("none");
    });
    wasOpen = isOpen;
  });

  function act(event: MouseEvent) {
    if (event.detail === 0) return session.addFieldFromKeyboard();
    session.setTool(drawing ? "none" : "drawField");
  }
</script>

<ToolSection
  tool="addField"
  {open}
  {onToggle}
  actionLabel={drawing ? "Stop Drawing" : "Draw Field"}
  pressed={drawing}
  effect={addFieldEffect(label, drawing)}
  onAction={act}
>
  <label class="setting">
    <span class="caption">Kind</span>
    <select class="field" bind:value={session.newKind}>
      {#each ADDABLE_KINDS as option (option.kind)}<option value={option.kind}>{option.label}</option>{/each}
    </select>
  </label>
  {#if needsChoices(session.newKind as AddableKind)}
    <label class="setting">
      <span class="caption">Choices, one per line</span>
      <textarea class="field" rows="3" bind:value={session.newChoices}></textarea>
    </label>
  {/if}
</ToolSection>

<script lang="ts">
  // Edit Fields: a mode apart from filling, where clicking a field selects
  // it instead of typing into it. the selected field gets the frame with
  // handles on the page (drag, or the arrows, to move it; Alt with the
  // arrows to resize; Delete to delete), and its settings show here.
  import { untrack } from "svelte";
  import type { ToolContext } from "../../inspector";
  import ToolSection from "../ToolSection.svelte";
  import FieldProperties from "./FieldProperties.svelte";

  interface Props {
    context: ToolContext;
    open: boolean;
    onToggle: () => void;
  }

  let { context, open, onToggle }: Props = $props();

  const session = $derived(context.fillSign!);
  const editing = $derived(session.mode === "edit");
  const selected = $derived(session.selectedField ? session.field(session.selectedField.field) : undefined);

  // closing the section goes back to filling
  let wasOpen = false;
  $effect(() => {
    const isOpen = open;
    untrack(() => {
      if (wasOpen && !isOpen && session.mode === "edit") session.setMode("fill");
    });
    wasOpen = isOpen;
  });

  function act(event: MouseEvent) {
    if (editing) return session.setMode("fill");
    session.setMode("edit");
    // from the keyboard: select the first field, so the arrows work at once
    const first = session.fields.find((field) => field.widgets.some((widget) => widget.page !== null));
    if (event.detail === 0 && first) session.selectField(first.name, 0);
  }
</script>

<ToolSection
  tool="editFields"
  {open}
  {onToggle}
  actionLabel={editing ? "Done Editing" : "Edit Fields"}
  pressed={editing}
  ready={session.fields.length > 0}
  effect={session.fields.length === 0
    ? "This document has no fields yet. Add Field or Find Fields… can add some."
    : editing
      ? "Click a field on the page to select it. Each change is one undo step in your working copy; the file on disk changes only when you choose Save."
      : "Shows every field as an outline with its name, ready to select."}
  onAction={act}
>
  {#if editing && selected}
    {#key selected.name}
      <FieldProperties {session} field={selected} />
    {/key}
  {:else if editing}
    <p class="hint">Click a field on the page to select it. Drag it to move it, or drag a handle to resize it.</p>
  {/if}
</ToolSection>

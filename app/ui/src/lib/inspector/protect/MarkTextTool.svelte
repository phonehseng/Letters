<script lang="ts">
  // Mark Text: with text selected the button marks it at once; without,
  // it picks the tool, and the next drag across words is marked. each
  // line becomes one marked area. nothing is removed until Apply.
  import { untrack } from "svelte";
  import type { ToolContext } from "../../inspector";
  import { isCollapsed, selectedPages } from "../../pageTools/textSelection";
  import { markTextEffect } from "../../protect/protectEffects";
  import type { ProtectSession } from "../../protect/protectSession.svelte";
  import ToolSection from "../ToolSection.svelte";

  interface Props {
    context: ToolContext;
    session: ProtectSession;
    open: boolean;
    onToggle: () => void;
  }

  let { context, session, open, onToggle }: Props = $props();

  const active = $derived(session.tool === "markText");
  const selection = $derived(context.annotate.selection);
  const hasSelection = $derived(!isCollapsed(selection));

  // closing the section puts its tool away (only a close does, as in Annotate)
  let wasOpen = false;
  $effect(() => {
    const isOpen = open;
    untrack(() => {
      if (wasOpen && !isOpen && session.tool === "markText") session.setTool(null);
    });
    wasOpen = isOpen;
  });

  function act() {
    if (hasSelection) {
      session.markSelection();
      return;
    }
    session.setTool(active ? null : "markText");
  }
</script>

<ToolSection
  tool="redactMarkText"
  {open}
  {onToggle}
  actionLabel={hasSelection ? "Mark Selected Text" : active ? "Stop Marking" : "Mark Text"}
  pressed={active}
  effect={markTextEffect(active, hasSelection && selection ? selectedPages(selection) : null, context.document.pages.length)}
  onAction={act}
/>

<script lang="ts">
  // Mark Area: drag boxes over anything on the page (pictures, scanned
  // text, a signature). the tool stays until Escape, so several boxes can
  // be drawn in a row. pressed from the keyboard, the button puts a box
  // in the middle of the page on screen and gives it the focus, so the
  // arrows move it and Alt with the arrows changes its size.
  import { untrack } from "svelte";
  import type { ToolContext } from "../../inspector";
  import { markAreaEffect } from "../../protect/protectEffects";
  import type { ProtectSession } from "../../protect/protectSession.svelte";
  import ToolSection from "../ToolSection.svelte";

  interface Props {
    context: ToolContext;
    session: ProtectSession;
    open: boolean;
    onToggle: () => void;
  }

  let { session, open, onToggle }: Props = $props();

  const active = $derived(session.tool === "markArea");

  let wasOpen = false;
  $effect(() => {
    const isOpen = open;
    untrack(() => {
      if (wasOpen && !isOpen && session.tool === "markArea") session.setTool(null);
    });
    wasOpen = isOpen;
  });

  function act(event: MouseEvent) {
    // a press from the keyboard has no pointer to draw with
    if (event.detail === 0) {
      session.setTool("markArea");
      session.markAreaFromKeyboard();
      return;
    }
    session.setTool(active ? null : "markArea");
  }
</script>

<ToolSection
  tool="redactMarkArea"
  {open}
  {onToggle}
  actionLabel={active ? "Stop Marking" : "Mark Area"}
  pressed={active}
  effect={markAreaEffect(active)}
  onAction={act}
/>

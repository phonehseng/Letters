<script lang="ts">
  // a sticky note: pick the tool and click the page, then type in the
  // pop-up. from the keyboard the button puts the note in the middle of
  // the page on screen and opens it for typing at once.
  import { untrack } from "svelte";
  import { placeEffect } from "../../annotate/annotateEffects";
  import { NOTE_SWATCHES } from "../../annotate/palette";
  import Swatches from "../../annotate/Swatches.svelte";
  import type { ToolContext } from "../../inspector";
  import ToolSection from "../ToolSection.svelte";

  interface Props {
    context: ToolContext;
    open: boolean;
    onToggle: () => void;
  }

  let { context, open, onToggle }: Props = $props();

  const session = $derived(context.annotate);
  const active = $derived(session.tool === "note");

  // closing the section puts its tool away. only a close does: a section
  // that starts closed (the tab just opened) leaves a tool picked by a shortcut alone
  let wasOpen = false;
  $effect(() => {
    const isOpen = open;
    untrack(() => {
      if (wasOpen && !isOpen && session.tool === "note") session.setTool("select");
    });
    wasOpen = isOpen;
  });

  function act(event: MouseEvent) {
    // a button pressed from the keyboard has no place on the page to click
    if (event.detail === 0) return session.placeFromKeyboard("note");
    session.setTool(active ? "select" : "note");
  }
</script>

<ToolSection
  tool="addNote"
  {open}
  {onToggle}
  actionLabel={active ? "Cancel" : "Add Note"}
  pressed={active}
  effect={placeEffect("a note", "Click where the note should go.", active)}
  onAction={act}
>
  <Swatches label="Colour" swatches={NOTE_SWATCHES} value={session.styles.note} onChange={(color) => color && (session.styles.note = color)} />
  <p class="hint">From the keyboard, press Add Note to put a note in the middle of the page and start typing.</p>
</ToolSection>

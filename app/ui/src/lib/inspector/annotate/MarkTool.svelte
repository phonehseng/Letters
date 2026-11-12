<script lang="ts">
  // Highlight, Underline and Strike Through: one section each, the same
  // component. with text selected the button marks it at once; without,
  // it picks the tool, and the next drag across text is marked.
  import { untrack } from "svelte";
  import type { TextMarkupKind } from "../../annotate/annotateApi";
  import { markEffect } from "../../annotate/annotateEffects";
  import { HIGHLIGHT_SWATCHES, INK_SWATCHES } from "../../annotate/palette";
  import Swatches from "../../annotate/Swatches.svelte";
  import type { ToolContext } from "../../inspector";
  import { isCollapsed, selectedPages } from "../../pageTools/textSelection";
  import type { ToolName } from "../../tools";
  import ToolSection from "../ToolSection.svelte";

  interface Props {
    context: ToolContext;
    kind: Exclude<TextMarkupKind, "squiggly">;
    open: boolean;
    onToggle: () => void;
  }

  let { context, kind, open, onToggle }: Props = $props();

  const NAMES: Record<string, { tool: ToolName; label: string; stop: string }> = {
    highlight: { tool: "highlightText", label: "Highlight", stop: "Stop Highlighting" },
    underline: { tool: "underlineText", label: "Underline", stop: "Stop Underlining" },
    strikeOut: { tool: "strikeText", label: "Strike Through", stop: "Stop Striking Through" },
  };

  const session = $derived(context.annotate);
  const names = $derived(NAMES[kind]);
  const active = $derived(session.tool === kind);
  const hasSelection = $derived(!isCollapsed(session.selection));

  // closing the section puts its tool away. only a close does: a section
  // that starts closed (the tab just opened) leaves a tool picked by a shortcut alone
  let wasOpen = false;
  $effect(() => {
    const isOpen = open;
    untrack(() => {
      if (wasOpen && !isOpen && session.tool === kind) session.setTool("select");
    });
    wasOpen = isOpen;
  });

  function act() {
    if (hasSelection) return session.applyMark(kind);
    session.setTool(active ? "select" : kind);
  }
</script>

<ToolSection
  tool={names.tool}
  {open}
  {onToggle}
  actionLabel={active && !hasSelection ? names.stop : names.label}
  pressed={active}
  effect={markEffect(kind, hasSelection && session.selection ? selectedPages(session.selection) : null, active)}
  onAction={act}
>
  <Swatches
    label="Colour"
    swatches={kind === "highlight" ? HIGHLIGHT_SWATCHES : INK_SWATCHES}
    value={session.styles.marks[kind]}
    onChange={(color) => color && (session.styles.marks[kind] = color)}
  />
</ToolSection>

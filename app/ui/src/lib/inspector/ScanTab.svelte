<script lang="ts">
  // the Scan tab of the inspector: reading the text of scanned pages
  // (OCR), making it real editable text, checking and correcting it,
  // removing it, and making a PDF from pictures. one disclosure section
  // open at a time, like the other tabs.
  import { untrack, type Component } from "svelte";
  import { appCommands } from "../commands";
  import type { InspectorTabProps } from "../inspector";
  import type { ToolName } from "../tools";
  import MakeEditableTool from "./scan/MakeEditableTool.svelte";
  import RecognizeTool from "./scan/RecognizeTool.svelte";
  import RemoveTextTool from "./scan/RemoveTextTool.svelte";
  import ShowTextTool from "./scan/ShowTextTool.svelte";
  import ToolSection from "./ToolSection.svelte";

  type SectionProps = InspectorTabProps & { open: boolean; onToggle: () => void };

  let { context }: InspectorTabProps = $props();

  const SECTIONS: { tool: ToolName; component: Component<SectionProps> }[] = [
    { tool: "recognizeText", component: RecognizeTool },
    { tool: "makeEditableText", component: MakeEditableTool },
    { tool: "showRecognizedText", component: ShowTextTool },
    { tool: "removeRecognizedText", component: RemoveTextTool },
  ];

  let openTool = $state<ToolName | null>("recognizeText");

  let lastRequest = -1;
  $effect(() => {
    const request = context.requested;
    if (!request || request.at === lastRequest) return;
    lastRequest = request.at;
    if (SECTIONS.some((section) => section.tool === request.tool) || request.tool === "newFromImages") untrack(() => (openTool = request.tool));
  });

  function toggle(tool: ToolName) {
    openTool = openTool === tool ? null : tool;
  }
</script>

<div class="sections">
  {#if context.scan}
    {#each SECTIONS as section (section.tool)}
      <section.component {context} open={openTool === section.tool} onToggle={() => toggle(section.tool)} />
    {/each}
    <ToolSection
      tool="newFromImages"
      open={openTool === "newFromImages"}
      onToggle={() => toggle("newFromImages")}
      actionLabel="New from Images…"
      effect="Opens a sheet to choose pictures and their order. This document doesn’t change."
      onAction={() => {
        appCommands.dispatch("new-from-images", "pointer");
      }}
    />
  {/if}
</div>

<style>
  .sections {
    display: flex;
    flex-direction: column;
  }
</style>

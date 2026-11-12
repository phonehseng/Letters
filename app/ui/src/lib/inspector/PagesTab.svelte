<script lang="ts">
  // the Pages tab of the inspector: every page tool as a disclosure
  // section, one open at a time, in the order a student is likely to
  // need them: turning and removing first, making new files after,
  // drawing on pages last.
  import type { Component } from "svelte";
  import type { InspectorTabProps } from "../inspector";
  import type { ToolName } from "../tools";
  import CropTool from "./pages/CropTool.svelte";
  import DeleteTool from "./pages/DeleteTool.svelte";
  import ExtractTool from "./pages/ExtractTool.svelte";
  import HeaderFooterTool from "./pages/HeaderFooterTool.svelte";
  import InsertTool from "./pages/InsertTool.svelte";
  import MergeTool from "./pages/MergeTool.svelte";
  import PageNumbersTool from "./pages/PageNumbersTool.svelte";
  import RemoveTool from "./pages/RemoveTool.svelte";
  import RotateTool from "./pages/RotateTool.svelte";
  import SplitTool from "./pages/SplitTool.svelte";
  import WatermarkTool from "./pages/WatermarkTool.svelte";

  type SectionProps = Omit<InspectorTabProps, never> & { open: boolean; onToggle: () => void };

  let { context }: InspectorTabProps = $props();

  // the order here matches the order in the registry (inspector.ts)
  const SECTIONS: [ToolName, Component<SectionProps>][] = [
    ["rotatePages", RotateTool],
    ["deletePages", DeleteTool],
    ["insertPages", InsertTool],
    ["extractPages", ExtractTool],
    ["splitDocument", SplitTool],
    ["mergeDocuments", MergeTool],
    ["cropPages", CropTool],
    ["pageNumbers", PageNumbersTool],
    ["headerFooter", HeaderFooterTool],
    ["watermark", WatermarkTool],
    ["removeStamps", RemoveTool],
  ];

  let openTool = $state<ToolName | null>(null);

  // a menu command (Pages > Crop…, say) opens its section
  $effect(() => {
    const request = context.requested;
    if (request && SECTIONS.some(([tool]) => tool === request.tool)) openTool = request.tool;
  });

  function toggle(tool: ToolName) {
    openTool = openTool === tool ? null : tool;
  }
</script>

<div class="sections">
  {#each SECTIONS as [tool, Section] (tool)}
    <Section {context} open={openTool === tool} onToggle={() => toggle(tool)} />
  {/each}
</div>

<style>
  .sections {
    display: flex;
    flex-direction: column;
  }
</style>

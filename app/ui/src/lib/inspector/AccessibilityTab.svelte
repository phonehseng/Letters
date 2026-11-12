<script lang="ts">
  // the Accessibility tab of the inspector: check the document, go through
  // the fixes one at a time, mark it as PDF/UA once that is honest, and
  // save the report. each is a disclosure section, one open at a time,
  // like the other tabs. the words about each rule come from the engine
  // (core/a11y), so the tab never says more than the checker found.
  import { untrack } from "svelte";
  import type { InspectorTabProps } from "../inspector";
  import type { ToolName } from "../tools";
  import CheckSection from "./accessibility/CheckSection.svelte";
  import FixSection from "./accessibility/FixSection.svelte";
  import PdfUaSection from "./accessibility/PdfUaSection.svelte";
  import ReportSection from "./accessibility/ReportSection.svelte";

  let { context }: InspectorTabProps = $props();

  const session = $derived(context.a11y);
  let openTool = $state<ToolName | null>("checkAccessibility");

  // a menu command (Check Accessibility…) opens its section, each request once
  let lastRequest = -1;
  $effect(() => {
    const request = context.requested;
    if (!request || request.at === lastRequest) return;
    lastRequest = request.at;
    const mine: ToolName[] = ["checkAccessibility", "fixAccessibility", "markPdfUa", "saveAccessibilityReport"];
    if (mine.includes(request.tool)) untrack(() => (openTool = request.tool));
  });

  // the engine's sentences (what tags and alt text are) are read once
  $effect(() => {
    untrack(() => session?.loadWords());
  });

  // an open wizard keeps its section open
  $effect(() => {
    if (session?.wizard) untrack(() => (openTool = "fixAccessibility"));
  });

  function toggle(tool: ToolName) {
    openTool = openTool === tool ? null : tool;
    // leaving the wizard's section puts away what it drew on the page
    if (openTool !== "fixAccessibility") session?.hideReadingOrder();
  }
</script>

{#if session}
  <div class="sections">
    <CheckSection {context} {session} open={openTool === "checkAccessibility"} onToggle={() => toggle("checkAccessibility")} onFix={() => (openTool = "fixAccessibility")} />
    <FixSection {context} {session} open={openTool === "fixAccessibility"} onToggle={() => toggle("fixAccessibility")} />
    <PdfUaSection {session} open={openTool === "markPdfUa"} onToggle={() => toggle("markPdfUa")} />
    <ReportSection {context} {session} open={openTool === "saveAccessibilityReport"} onToggle={() => toggle("saveAccessibilityReport")} />
  </div>
{/if}

<style>
  .sections {
    display: flex;
    flex-direction: column;
  }
</style>

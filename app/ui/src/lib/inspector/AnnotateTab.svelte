<script lang="ts">
  // the Annotate tab of the inspector: marking text, notes, text boxes,
  // shapes, drawing and pictures, then the comments list and flattening.
  // each is a disclosure section, one open at a time, like the Pages tab.
  //
  // the tools work on the page itself. a section's button picks its page
  // tool (or, for the text marks, marks the selected text at once); the
  // tool stays until Escape, until the section closes, or until the
  // button is pressed again. it keeps working when the inspector is
  // closed, which on a narrow window is the only way to see the page. clicking a comment or mark on the page opens
  // the Selection section, where it can be restyled or deleted.
  import { untrack, type Component } from "svelte";
  import type { TextMarkupKind } from "../annotate/annotateApi";
  import type { InspectorTabProps } from "../inspector";
  import type { ToolName } from "../tools";
  import AuthorTool from "./annotate/AuthorTool.svelte";
  import CommentsTool from "./annotate/CommentsTool.svelte";
  import DrawTool from "./annotate/DrawTool.svelte";
  import FlattenTool from "./annotate/FlattenTool.svelte";
  import MarkTool from "./annotate/MarkTool.svelte";
  import NoteTool from "./annotate/NoteTool.svelte";
  import PictureTool from "./annotate/PictureTool.svelte";
  import SelectionTool from "./annotate/SelectionTool.svelte";
  import ShapesTool from "./annotate/ShapesTool.svelte";
  import TextBoxTool from "./annotate/TextBoxTool.svelte";

  type SectionProps = InspectorTabProps & { open: boolean; onToggle: () => void };
  type Section = { tool: ToolName; component: Component<SectionProps> } | { tool: ToolName; mark: Exclude<TextMarkupKind, "squiggly"> };

  let { context }: InspectorTabProps = $props();

  // the order here matches the order in the registry (inspector.ts)
  const SECTIONS: Section[] = [
    { tool: "editAnnotation", component: SelectionTool },
    { tool: "highlightText", mark: "highlight" },
    { tool: "underlineText", mark: "underline" },
    { tool: "strikeText", mark: "strikeOut" },
    { tool: "addNote", component: NoteTool },
    { tool: "addTextBox", component: TextBoxTool },
    { tool: "drawShapes", component: ShapesTool },
    { tool: "drawInk", component: DrawTool },
    { tool: "addPicture", component: PictureTool },
    { tool: "comments", component: CommentsTool },
    { tool: "flattenComments", component: FlattenTool },
    { tool: "authorName", component: AuthorTool },
  ];

  let openTool = $state<ToolName | null>(null);

  // a menu command (Annotate > Flatten Comments…, say) opens its section
  // (each request once: the context changes on every scroll)
  let lastRequest = -1;
  $effect(() => {
    const request = context.requested;
    if (!request || request.at === lastRequest) return;
    lastRequest = request.at;
    if (SECTIONS.some((section) => section.tool === request.tool)) untrack(() => (openTool = request.tool));
  });

  // clicking a comment or mark on the page opens the section that changes
  // it. only a new click does: the context changes on every scroll, and
  // the section must stay closed once the person closed it
  let lastPicks = untrack(() => context.annotate.picks);
  $effect(() => {
    const picks = context.annotate.picks;
    if (picks !== lastPicks && context.annotate.selectedId) untrack(() => (openTool = "editAnnotation"));
    lastPicks = picks;
  });

  function toggle(tool: ToolName) {
    openTool = openTool === tool ? null : tool;
  }
</script>

<div class="sections">
  {#each SECTIONS as section (section.tool)}
    {#if "mark" in section}
      <MarkTool {context} kind={section.mark} open={openTool === section.tool} onToggle={() => toggle(section.tool)} />
    {:else}
      <section.component {context} open={openTool === section.tool} onToggle={() => toggle(section.tool)} />
    {/if}
  {/each}
</div>

<style>
  .sections {
    display: flex;
    flex-direction: column;
  }
</style>

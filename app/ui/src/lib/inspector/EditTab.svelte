<script lang="ts">
  // the Edit tab of the inspector: changing the page's own text in place,
  // its style, deleting and adding text, and the page's pictures. each is
  // a disclosure section, one open at a time, like the other tabs.
  //
  // the tools work on the page itself (see edit/editSession.svelte.ts). a
  // section's button picks its page tool; it stays until Escape, until the
  // section closes, or until the button is pressed again, and it keeps
  // working when the inspector is closed, which on a narrow window is the
  // only way to see the page.
  import { untrack, type Component } from "svelte";
  import type { InspectorTabProps } from "../inspector";
  import type { ToolName } from "../tools";
  import AddImageTool from "./edit/AddImageTool.svelte";
  import AddTextTool from "./edit/AddTextTool.svelte";
  import DeleteTextTool from "./edit/DeleteTextTool.svelte";
  import EditTextTool from "./edit/EditTextTool.svelte";
  import PicturesTool from "./edit/PicturesTool.svelte";
  import TextStyleTool from "./edit/TextStyleTool.svelte";

  type SectionProps = InspectorTabProps & { open: boolean; onToggle: () => void };

  let { context }: InspectorTabProps = $props();

  // the order here matches the order in the registry (inspector.ts)
  const SECTIONS: { tool: ToolName; component: Component<SectionProps> }[] = [
    { tool: "editText", component: EditTextTool },
    { tool: "editTextStyle", component: TextStyleTool },
    { tool: "deletePageText", component: DeleteTextTool },
    { tool: "addPageText", component: AddTextTool },
    { tool: "pageImages", component: PicturesTool },
    { tool: "addPageImage", component: AddImageTool },
  ];

  let openTool = $state<ToolName | null>("editText");

  // a menu command (Content > Add Text, say) opens its section, each request once
  let lastRequest = -1;
  $effect(() => {
    const request = context.requested;
    if (!request || request.at === lastRequest) return;
    lastRequest = request.at;
    if (SECTIONS.some((section) => section.tool === request.tool)) untrack(() => (openTool = request.tool));
  });

  // clicking a picture on the page opens Pictures; clicking into text opens Edit Text
  $effect(() => {
    const edit = context.edit;
    if (!edit) return;
    const picked = edit.pictures.selected;
    const typing = edit.editor?.block;
    untrack(() => {
      if (picked && openTool !== "pageImages") openTool = "pageImages";
      else if (typing && openTool !== "editText" && openTool !== "editTextStyle" && openTool !== "deletePageText") openTool = "editText";
    });
  });

  function toggle(tool: ToolName) {
    openTool = openTool === tool ? null : tool;
  }
</script>

<div class="sections">
  {#if context.edit}
    {#each SECTIONS as section (section.tool)}
      <section.component {context} open={openTool === section.tool} onToggle={() => toggle(section.tool)} />
    {/each}
  {/if}
</div>

<style>
  .sections {
    display: flex;
    flex-direction: column;
  }
</style>

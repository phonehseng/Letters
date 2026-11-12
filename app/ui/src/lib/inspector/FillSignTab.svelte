<script lang="ts">
  // the Fill & Sign tab of the inspector: filling a form, finding fields
  // on a flat one, adding and editing fields, flattening, and signing with
  // a picture or a certificate, then the list of signatures. each is a
  // disclosure section, one open at a time, like the other tabs.
  //
  // filling happens on the page itself: the fields are tinted boxes to
  // click or Tab into (see forms/FormsOverlay.svelte). the tab holds the
  // field list, the settings and the explanations. everything stays on
  // this computer: there is no server and nothing records who filled in
  // or signed a form.
  import { untrack, type Component } from "svelte";
  import type { InspectorTabProps } from "../inspector";
  import type { ToolName } from "../tools";
  import AddFieldTool from "./fillsign/AddFieldTool.svelte";
  import AuditTrailTool from "./fillsign/AuditTrailTool.svelte";
  import EditFieldsTool from "./fillsign/EditFieldsTool.svelte";
  import FillFormTool from "./fillsign/FillFormTool.svelte";
  import FindFieldsTool from "./fillsign/FindFieldsTool.svelte";
  import FlattenFormTool from "./fillsign/FlattenFormTool.svelte";
  import FormDataTool from "./fillsign/FormDataTool.svelte";
  import PrepareFormTool from "./fillsign/PrepareFormTool.svelte";
  import SignaturesTool from "./fillsign/SignaturesTool.svelte";
  import SignCertificateTool from "./fillsign/SignCertificateTool.svelte";
  import SignPictureTool from "./fillsign/SignPictureTool.svelte";

  type SectionProps = InspectorTabProps & { open: boolean; onToggle: () => void };

  let { context }: InspectorTabProps = $props();

  // the order here matches the order in the registry (inspector.ts)
  const SECTIONS: { tool: ToolName; component: Component<SectionProps> }[] = [
    { tool: "fillForm", component: FillFormTool },
    { tool: "findFields", component: FindFieldsTool },
    { tool: "addField", component: AddFieldTool },
    { tool: "editFields", component: EditFieldsTool },
    { tool: "prepareForm", component: PrepareFormTool },
    { tool: "formData", component: FormDataTool },
    { tool: "flattenForm", component: FlattenFormTool },
    { tool: "signWithPicture", component: SignPictureTool },
    { tool: "signWithCertificate", component: SignCertificateTool },
    { tool: "signatures", component: SignaturesTool },
    { tool: "auditTrail", component: AuditTrailTool },
  ];

  // a document with fields opens on Fill Form, the reason most people come here
  let openTool = $state<ToolName | null>(untrack(() => ((context.fillSign?.fields.length ?? 0) > 0 ? "fillForm" : null)));

  // a menu command (Forms > Flatten Form…, say) opens its section, each request once
  let lastRequest = -1;
  $effect(() => {
    const request = context.requested;
    if (!request || request.at === lastRequest) return;
    lastRequest = request.at;
    const section = request.tool === "nextEmptyField" || request.tool === "showFields" ? "fillForm" : request.tool;
    const target = section === "selfSignedCertificate" ? "signWithCertificate" : section;
    if (SECTIONS.some((each) => each.tool === target)) untrack(() => (openTool = target));
  });

  function toggle(tool: ToolName) {
    openTool = openTool === tool ? null : tool;
  }
</script>

<div class="sections">
  {#if context.fillSign}
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

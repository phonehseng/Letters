<script lang="ts">
  // the Protect tab of the inspector: redaction (mark, find and mark,
  // review, apply), removing hidden information, and passwords. each is
  // a disclosure section, one open at a time, like the other tabs.
  //
  // redaction is two steps on purpose: marks are only drawn on screen
  // (outlined, hatched) until Apply Redactions, because applying can't be
  // taken back once the file is saved and shared. the line at the top says
  // how the file is locked and when a new password takes effect.
  import { untrack, type Component } from "svelte";
  import type { InspectorTabProps, ToolContext } from "../inspector";
  import type { ProtectSession } from "../protect/protectSession.svelte";
  import type { ToolName } from "../tools";
  import ApplyTool from "./protect/ApplyTool.svelte";
  import FindMarkTool from "./protect/FindMarkTool.svelte";
  import LockStatus from "./protect/LockStatus.svelte";
  import MarkAreaTool from "./protect/MarkAreaTool.svelte";
  import MarkTextTool from "./protect/MarkTextTool.svelte";
  import OpenPasswordTool from "./protect/OpenPasswordTool.svelte";
  import RemovePasswordTool from "./protect/RemovePasswordTool.svelte";
  import RestrictTool from "./protect/RestrictTool.svelte";
  import ReviewTool from "./protect/ReviewTool.svelte";
  import ScrubTool from "./protect/ScrubTool.svelte";
  import UnlockTool from "./protect/UnlockTool.svelte";

  type SectionProps = { context: ToolContext; session: ProtectSession; open: boolean; onToggle: () => void };

  let { context }: InspectorTabProps = $props();

  // the order here matches the order in the registry (inspector.ts)
  const SECTIONS: { tool: ToolName; component: Component<SectionProps>; onlyWhenLocked?: boolean }[] = [
    { tool: "redactMarkText", component: MarkTextTool },
    { tool: "redactMarkArea", component: MarkAreaTool },
    { tool: "redactFind", component: FindMarkTool },
    { tool: "redactReview", component: ReviewTool },
    { tool: "redactApply", component: ApplyTool },
    { tool: "removeHiddenInfo", component: ScrubTool },
    { tool: "unlockForEditing", component: UnlockTool, onlyWhenLocked: true },
    { tool: "openPassword", component: OpenPasswordTool },
    { tool: "restrictPermissions", component: RestrictTool },
    { tool: "removePassword", component: RemovePasswordTool },
  ];

  const session = $derived(context.protect);
  const locked = $derived(session?.status?.locked ?? false);
  let openTool = $state<ToolName | null>(null);

  // a locked document opens at Unlock for Editing, the one thing it needs first
  let showedUnlock = false;
  $effect(() => {
    if (locked && !showedUnlock) {
      showedUnlock = true;
      untrack(() => (openTool = "unlockForEditing"));
    }
  });

  // a menu command (Protect > Find and Mark…, say) opens its section,
  // each request once: the context changes on every scroll
  let lastRequest = -1;
  $effect(() => {
    const request = context.requested;
    if (!request || request.at === lastRequest) return;
    lastRequest = request.at;
    if (SECTIONS.some((section) => section.tool === request.tool)) untrack(() => (openTool = request.tool));
  });

  // the status is read when the tab opens, so the lock line is current
  $effect(() => {
    untrack(() => session?.refreshStatus());
  });

  function toggle(tool: ToolName) {
    openTool = openTool === tool ? null : tool;
  }
</script>

{#if session}
  <LockStatus status={session.status} />
  <div class="sections">
    {#each SECTIONS as section (section.tool)}
      {#if !section.onlyWhenLocked || locked}
        <section.component {context} {session} open={openTool === section.tool} onToggle={() => toggle(section.tool)} />
      {/if}
    {/each}
  </div>
{/if}

<style>
  .sections {
    display: flex;
    flex-direction: column;
  }
</style>

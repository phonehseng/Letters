<script lang="ts">
  // Remove Password: the file is saved with no password and no
  // restrictions. for a document that was unlocked, or that was given a
  // password in this session.
  import type { ToolContext } from "../../inspector";
  import { hasProtection, removePasswordEffect } from "../../protect/protectEffects";
  import type { ProtectSession } from "../../protect/protectSession.svelte";
  import ToolSection from "../ToolSection.svelte";

  interface Props {
    context: ToolContext;
    session: ProtectSession;
    open: boolean;
    onToggle: () => void;
  }

  let { session, open, onToggle }: Props = $props();

  const locked = $derived(session.status?.locked ?? false);
</script>

<ToolSection
  tool="removePassword"
  {open}
  {onToggle}
  actionLabel="Remove Password"
  ready={hasProtection(session.status) && !locked}
  effect={locked ? "Unlock the document first." : removePasswordEffect(session.status)}
  onAction={() => session.changeLock({ removeAll: true }, "The file will be saved without a password.")}
/>

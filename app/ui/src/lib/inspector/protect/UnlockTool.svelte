<script lang="ts">
  // Unlock for Editing: a locked PDF can be read with its password, but
  // changing it needs an unlocked working copy. the password unlocks it;
  // saving locks it again the way it was locked, unless the person
  // changes that below. a file that only has restrictions, and allows
  // changes, needs no password at all.
  import type { ToolContext } from "../../inspector";
  import { unlockEffect } from "../../protect/protectEffects";
  import type { ProtectSession } from "../../protect/protectSession.svelte";
  import ToolSection from "../ToolSection.svelte";

  interface Props {
    context: ToolContext;
    session: ProtectSession;
    open: boolean;
    onToggle: () => void;
  }

  let { session, open, onToggle }: Props = $props();

  let password = $state("");
  let show = $state(false);

  const lock = $derived(session.status?.lock ?? null);
  const needsPassword = $derived(lock ? lock.needsOpenPassword || !lock.permissions.editing : true);

  async function unlock() {
    if (await session.unlock(password)) password = "";
  }
</script>

<ToolSection
  tool="unlockForEditing"
  {open}
  {onToggle}
  actionLabel="Unlock for Editing"
  ready={!needsPassword || password.length > 0}
  effect={unlockEffect(session.status)}
  onAction={unlock}
>
  {#if needsPassword}
    <form
      class="setting"
      onsubmit={(event) => {
        event.preventDefault();
        unlock();
      }}
    >
      <label class="caption" for="unlock-password">{lock?.needsOpenPassword ? "Password" : "Owner password"}</label>
      <input id="unlock-password" class="field" type={show ? "text" : "password"} autocomplete="off" bind:value={password} />
      <label class="check">
        <input type="checkbox" bind:checked={show} />
        <span>Show password</span>
      </label>
    </form>
  {/if}
</ToolSection>

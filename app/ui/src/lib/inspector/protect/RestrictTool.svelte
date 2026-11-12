<script lang="ts">
  // Restrict What Others Can Do: printing, copying, changes, comments,
  // forms and pages, and an owner password that lifts the restrictions.
  // one sentence says what this really is: a request other apps may
  // ignore, not a lock. the restrictions are applied when the file is saved.
  import { untrack } from "svelte";
  import type { ToolContext } from "../../inspector";
  import { ALL_ALLOWED, isRestricted, PERMISSION_ROWS, PRINT_CHOICES, restrictionSentence } from "../../protect/permissions";
  import type { PermissionOptions, PrintPermission } from "../../protect/protectApi";
  import { RESTRICTIONS_HONESTY, restrictEffect } from "../../protect/protectEffects";
  import type { ProtectSession } from "../../protect/protectSession.svelte";
  import Choices from "../Choices.svelte";
  import ToolSection from "../ToolSection.svelte";

  interface Props {
    context: ToolContext;
    session: ProtectSession;
    open: boolean;
    onToggle: () => void;
  }

  let { session, open, onToggle }: Props = $props();

  let permissions = $state<PermissionOptions>({ ...ALL_ALLOWED });
  let printing = $state<PrintPermission>("full");
  let owner = $state("");
  let show = $state(false);

  // the form starts from what save will apply now (kept after unlocking, say)
  $effect(() => {
    const current = session.status?.restrictionsOnSave ?? null;
    untrack(() => {
      permissions = { ...(current ?? ALL_ALLOWED) };
      printing = permissions.printing;
    });
  });

  const chosen = $derived<PermissionOptions>({ ...permissions, printing });
  const restricted = $derived(isRestricted(chosen));
  const locked = $derived(session.status?.locked ?? false);

  async function restrict() {
    const change = owner ? { permissions: chosen, ownerPassword: owner } : { permissions: chosen };
    if (await session.changeLock(change, "The restrictions will be added when you save.")) owner = "";
  }
</script>

<ToolSection
  tool="restrictPermissions"
  {open}
  {onToggle}
  actionLabel="Restrict"
  ready={restricted && !locked}
  effect={locked ? "Unlock the document first." : restrictEffect(restrictionSentence(chosen), restricted)}
  onAction={restrict}
>
  <p class="hint">{RESTRICTIONS_HONESTY}</p>
  <Choices label="Printing" options={PRINT_CHOICES} bind:value={printing} />
  <fieldset class="setting">
    <legend class="caption">Allowed</legend>
    {#each PERMISSION_ROWS as row (row.key)}
      <label class="check">
        <input type="checkbox" bind:checked={permissions[row.key]} />
        <span>{row.label}</span>
      </label>
    {/each}
  </fieldset>
  <div class="setting">
    <label class="caption" for="owner-password">Owner password (optional)</label>
    <input id="owner-password" class="field" type={show ? "text" : "password"} autocomplete="new-password" bind:value={owner} aria-describedby="owner-password-hint" />
    <span id="owner-password-hint" class="hint">The password that lifts the restrictions. Leave it empty and Letters makes one up that nobody knows, so nobody can lift them, you included.</span>
  </div>
  <label class="check">
    <input type="checkbox" bind:checked={show} />
    <span>Show password</span>
  </label>
</ToolSection>

<style>
  fieldset {
    margin: 0;
    padding: 0;
    border: 0;
  }
</style>

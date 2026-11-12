<script lang="ts">
  // Require a Password to Open: the password is typed twice, with a hint
  // about how hard it is to guess and a way to see what was typed. it is
  // applied when the file is saved; the one sentence under the fields
  // says plainly that this really encrypts and can't be undone without it.
  import type { ToolContext } from "../../inspector";
  import { canUsePassword, confirmHint, passwordStrength } from "../../protect/passwordStrength";
  import { OPEN_PASSWORD_HONESTY, openPasswordEffect } from "../../protect/protectEffects";
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
  let confirm = $state("");
  let show = $state(false);

  const strength = $derived(passwordStrength(password));
  const matches = $derived(confirmHint(password, confirm));
  const ready = $derived(canUsePassword(password, confirm));
  const locked = $derived(session.status?.locked ?? false);
  const replacing = $derived(Boolean(session.status?.openPasswordOnSave || session.status?.undecided));

  async function setPassword() {
    if (await session.changeLock({ openPassword: password }, "The password will be added when you save.")) {
      password = "";
      confirm = "";
    }
  }
</script>

<ToolSection
  tool="openPassword"
  {open}
  {onToggle}
  actionLabel={replacing ? "Change Password" : "Add Password"}
  ready={ready && !locked}
  effect={locked ? "Unlock the document first." : openPasswordEffect(ready, replacing)}
  onAction={setPassword}
>
  <p class="hint">{OPEN_PASSWORD_HONESTY}</p>
  {#if session.status?.undecided}
    <p class="hint">It was unlocked with its owner password, so Letters doesn’t know the password it had. Type one here to keep it locked, or choose Remove Password.</p>
  {/if}
  <div class="setting">
    <label class="caption" for="open-password">Password</label>
    <input id="open-password" class="field" type={show ? "text" : "password"} autocomplete="new-password" bind:value={password} aria-describedby="open-password-strength" />
    <span id="open-password-strength" class="hint strength {strength.level}" aria-live="polite">{strength.hint}</span>
  </div>
  <div class="setting">
    <label class="caption" for="open-password-confirm">Verify</label>
    <input id="open-password-confirm" class="field" type={show ? "text" : "password"} autocomplete="new-password" bind:value={confirm} aria-describedby="open-password-match" />
    <span id="open-password-match" class="hint" aria-live="polite">{matches}</span>
  </div>
  <label class="check">
    <input type="checkbox" bind:checked={show} />
    <span>Show password</span>
  </label>
</ToolSection>

<style>
  .strength.weak {
    color: var(--danger);
  }
</style>

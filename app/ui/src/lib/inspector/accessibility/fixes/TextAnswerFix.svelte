<script lang="ts">
  // a fix whose answer is a few words: a link's description, a form
  // field's tooltip, what a comment says.
  import type { FixRequest } from "../../../a11y/a11yApi";
  import type { A11ySession } from "../../../a11y/a11ySession.svelte";

  interface Props {
    session: A11ySession;
    label: string;
    hint: string;
    suggested: string | null;
    actionLabel: string;
    request: (text: string) => FixRequest;
  }

  let { session, label, hint, suggested, actionLabel, request }: Props = $props();

  // the suggestion fills the field once, then it is the person's
  // svelte-ignore state_referenced_locally
  let text = $state(suggested ?? "");
  let busy = $state(false);

  async function apply() {
    busy = true;
    await session.fix(request(text.trim()));
    busy = false;
  }
</script>

<label class="setting">
  <span class="caption">{label}</span>
  <input class="field" bind:value={text} />
</label>
<p class="hint">{hint}</p>
<div class="actions">
  <button class="button primary" disabled={!text.trim() || busy} onclick={apply}>{actionLabel}</button>
</div>

<style>
  .actions {
    display: flex;
    justify-content: flex-end;
  }
</style>

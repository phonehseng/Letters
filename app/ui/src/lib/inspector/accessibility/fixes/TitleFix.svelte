<script lang="ts">
  // the document's title, which screen readers say first and windows show
  // instead of the file name. the engine suggests the first heading.
  import type { A11ySession } from "../../../a11y/a11ySession.svelte";

  interface Props {
    session: A11ySession;
    suggested: string | null;
  }

  let { session, suggested }: Props = $props();

  // the suggestion fills the field once, then it is the person's
  // svelte-ignore state_referenced_locally
  let title = $state(suggested ?? "");
  let busy = $state(false);

  async function apply() {
    busy = true;
    await session.fix({ kind: "setTitle", title: title.trim() });
    busy = false;
  }
</script>

<label class="setting">
  <span class="caption">Title</span>
  <input class="field" bind:value={title} placeholder="such as Rivers of Europe" />
</label>
<div class="actions">
  <button class="button primary" disabled={!title.trim() || busy} onclick={apply}>Set Title</button>
</div>
<p class="hint">Sets the title and shows it instead of the file name. Undo with Ctrl Z.</p>

<style>
  .actions {
    display: flex;
    justify-content: flex-end;
  }
</style>

<script lang="ts">
  // Add Tags Automatically: tags built from the layout for a document that
  // has none, with its language set at the same time. afterwards the
  // decisions the tagger was unsure about are listed to look over.
  import type { A11ySession } from "../../../a11y/a11ySession.svelte";
  import { LANGUAGES, startingLanguage } from "../../../a11y/languages";
  import { tooltipFor } from "../../../tools";

  interface Props {
    session: A11ySession;
  }

  let { session }: Props = $props();

  let language = $state(startingLanguage(null, navigator.language));
  let setLanguage = $state(true);
  let busy = $state(false);

  async function apply() {
    busy = true;
    await session.fix({ kind: "addTags", language: setLanguage ? language : null });
    busy = false;
  }
</script>

<p class="hint">{session.words?.tags ?? ""}</p>
<label class="check">
  <input type="checkbox" bind:checked={setLanguage} />
  <span>Also set the document’s language</span>
</label>
{#if setLanguage}
  <label class="setting">
    <span class="caption">Language of the document</span>
    <select class="field" bind:value={language}>
      {#each LANGUAGES as choice (choice.code)}
        <option value={choice.code}>{choice.name}</option>
      {/each}
    </select>
  </label>
{/if}
<div class="actions">
  <button class="button primary" title={tooltipFor("addTagsAutomatically")} disabled={busy} onclick={apply}>Add Tags Automatically</button>
</div>
<p class="hint">Adds headings, paragraphs, lists, pictures and tables as tags. The pages look exactly the same. Undo with Ctrl Z.</p>

<style>
  .actions {
    display: flex;
    justify-content: flex-end;
  }
</style>

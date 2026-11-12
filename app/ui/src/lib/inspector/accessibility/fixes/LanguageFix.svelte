<script lang="ts">
  // the document's language, picked by its name. screen readers need it
  // to pronounce the words; "Other" takes a code for a language not listed.
  import type { A11ySession } from "../../../a11y/a11ySession.svelte";
  import { languageName, LANGUAGES, looksLikeLanguageCode, startingLanguage } from "../../../a11y/languages";

  interface Props {
    session: A11ySession;
    suggested: string | null;
  }

  let { session, suggested }: Props = $props();

  const OTHER = "other";
  // the suggestion picks the starting choice once, then it is the person's
  // svelte-ignore state_referenced_locally
  let choice = $state(startingLanguage(suggested, navigator.language));
  let typed = $state("");
  let busy = $state(false);

  const code = $derived(choice === OTHER ? typed.trim() : choice);
  const valid = $derived(looksLikeLanguageCode(code));

  async function apply() {
    busy = true;
    await session.fix({ kind: "setLanguage", language: code });
    busy = false;
  }
</script>

<label class="setting">
  <span class="caption">Language of the document</span>
  <select class="field" bind:value={choice}>
    {#each LANGUAGES as language (language.code)}
      <option value={language.code}>{language.name}</option>
    {/each}
    <option value={OTHER}>Other…</option>
  </select>
</label>
{#if choice === OTHER}
  <label class="setting">
    <span class="caption">Language code</span>
    <input class="field" bind:value={typed} placeholder="such as nl-BE" aria-describedby="language-code-hint" />
    <span class="hint" id="language-code-hint">A code like en-US, fr or zh-Hans.</span>
  </label>
{/if}
<div class="actions">
  <button class="button primary" disabled={!valid || busy} onclick={apply}>Set Language</button>
</div>
<p class="hint">{valid ? `Sets the language to ${languageName(code)} (${code}). Undo with Ctrl Z.` : "Type a language code to go on."}</p>

<style>
  .actions {
    display: flex;
    justify-content: flex-end;
  }
</style>

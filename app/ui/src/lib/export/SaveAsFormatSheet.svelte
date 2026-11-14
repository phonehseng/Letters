<script lang="ts">
  // Save As, first step: which format. the formats are the editor's own
  // list (saveAsFormat.svelte.ts); one this computer can't make stays in
  // the list, greyed, saying why. Continue opens the save panel with that
  // one type. Enter continues and Escape cancels.
  import { onMount } from "svelte";
  import { saveAsFormat } from "./saveAsFormat.svelte";

  const request = $derived(saveAsFormat.current);
  let chosen = $state(saveAsFormat.current?.chosen ?? "");
  let sheet = $state<HTMLDivElement>();
  const option = $derived(request?.options.find((each) => each.id === chosen) ?? null);

  function choose() {
    if (option?.available) request?.answer(option);
  }

  function onKeyDown(event: KeyboardEvent) {
    if (event.key === "Escape") {
      event.stopPropagation();
      request?.answer(null);
    } else if (event.key === "Enter") {
      event.preventDefault();
      choose();
    }
  }

  // the keyboard starts on the chosen format, so the arrows move through the list
  onMount(() => sheet?.querySelector<HTMLInputElement>("input:checked")?.focus());
</script>

{#if request}
  <div class="backdrop" role="presentation" onkeydown={onKeyDown}>
    <div class="glass sheet" bind:this={sheet} role="dialog" aria-modal="true" aria-labelledby="save-as-title">
      <h2 id="save-as-title">Save As</h2>
      <div class="columns">
        <fieldset>
          <legend class="label">Format</legend>
          {#each request.options as each (each.id)}
            <label class="choice" class:later={!each.available}>
              <input type="radio" name="save-as-format" value={each.id} bind:group={chosen} disabled={!each.available} />
              <span>{each.label}{#if each.extension}&nbsp;<span class="extension">.{each.extension}</span>{/if}</span>
            </label>
          {/each}
        </fieldset>
        {#if option}<p class="about">{option.note}</p>{/if}
      </div>
      <div class="buttons">
        <button class="button" onclick={() => request?.answer(null)}>Cancel</button>
        <button class="button primary" onclick={choose} disabled={!option?.available}>Continue…</button>
      </div>
    </div>
  </div>
{/if}

<style>
  .backdrop {
    position: fixed;
    inset: 0;
    z-index: 25;
    display: flex;
    justify-content: center;
    align-items: flex-start;
    padding-top: var(--toolbar-space);
    background: rgb(0 0 0 / 0.12);
  }

  .sheet {
    width: min(32rem, calc(100% - 2rem));
    padding: 1rem 1.25rem;
  }

  h2 {
    margin: 0 0 0.75rem;
    font: var(--title);
    letter-spacing: var(--title-spacing);
  }

  .columns {
    display: flex;
    flex-wrap: wrap;
    gap: 1rem;
  }

  fieldset {
    flex: 1 1 13rem;
    margin: 0;
    padding: 0;
    border: 0;
  }

  .label {
    padding: 0;
    margin-bottom: 0.25rem;
    font: var(--subheadline);
    color: var(--label-secondary);
  }

  .choice {
    display: flex;
    align-items: baseline;
    gap: 0.5rem;
    padding: 0.25rem 0;
    font: var(--body);
  }

  .extension {
    color: var(--label-secondary);
  }

  .later {
    color: var(--label-tertiary);
  }

  .about {
    flex: 1 1 11rem;
    margin: 1.5rem 0 0;
    font: var(--body);
    color: var(--label-secondary);
  }

  .buttons {
    display: flex;
    justify-content: flex-end;
    gap: 0.5rem;
    margin-top: 1rem;
  }
</style>

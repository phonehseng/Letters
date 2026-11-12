<script lang="ts">
  // Recents' New Spreadsheet…: a small sheet with a blank workbook and
  // the engine's starter workbooks (help::presets), each with a line on
  // what it holds. the chosen one opens unsaved; Save asks where.
  import { onMount } from "svelte";
  import { appCommands } from "../commands";
  import { chooseStarter, STARTERS } from "./sheetsCommands";

  interface Props {
    onClose: () => void;
  }

  let { onClose }: Props = $props();
  let dialog = $state<HTMLDivElement>();

  function choose(template: string | null) {
    chooseStarter(template);
    onClose();
    appCommands.dispatch("new-spreadsheet", "pointer");
  }

  onMount(() => dialog?.querySelector("button")?.focus());
</script>

<div class="backdrop" role="presentation" onclick={onClose}></div>
<!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
<div class="glass sheet" role="dialog" aria-modal="true" aria-label="New Spreadsheet" tabindex="-1" bind:this={dialog} onkeydown={(event) => event.key === "Escape" && onClose()}>
  <h2>New Spreadsheet</h2>
  <p>Start empty, or from a workbook already laid out with formulas and formats to learn from. Nothing is saved until you choose Save.</p>
  <ul>
    {#each STARTERS as starter (starter.name)}
      <li>
        <button class="choice" onclick={() => choose(starter.template)}>
          <strong>{starter.name}</strong>
          <span>{starter.about}</span>
        </button>
      </li>
    {/each}
  </ul>
  <div class="buttons"><button class="button" onclick={onClose}>Cancel</button></div>
</div>

<style>
  .backdrop {
    position: fixed;
    inset: 0;
    z-index: 30;
    background: rgb(0 0 0 / 0.2);
  }

  .sheet {
    position: fixed;
    top: 15vh;
    left: 50%;
    z-index: 31;
    width: min(30rem, calc(100% - 2rem));
    padding: 1rem 1.25rem;
    border-radius: var(--radius-group);
    transform: translateX(-50%);
  }

  h2 {
    margin: 0;
    font: var(--title);
  }

  p {
    color: var(--label-secondary);
    font: var(--body);
  }

  ul {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(8.5rem, 1fr));
    gap: 0.5rem;
    margin: 0;
    padding: 0;
    list-style: none;
  }

  .choice {
    display: flex;
    flex-direction: column;
    gap: 0.25rem;
    width: 100%;
    height: 100%;
    padding: 0.625rem 0.75rem;
    border: 0;
    border-radius: var(--radius-control);
    background: var(--control);
    color: var(--label);
    text-align: left;
  }

  .choice:hover {
    background: var(--control-hover);
  }

  .choice span {
    color: var(--label-secondary);
    font: var(--subheadline);
  }

  .buttons {
    display: flex;
    justify-content: flex-end;
    margin-top: 0.75rem;
  }
</style>

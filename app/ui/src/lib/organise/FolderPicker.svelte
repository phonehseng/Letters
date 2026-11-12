<script lang="ts">
  // "Move to Folder…" and "Copy to…": a quick list of pinned folders,
  // search folders and recently used places, filtered as you type, with
  // "Choose Another Folder…" at the end.
  import { onMount } from "svelte";
  import { chooseFolder } from "../library/libraryApi";
  import { destinations, nameOf, type Destination } from "./organise";

  interface Props {
    title: string;
    pinned: string[];
    library: string[];
    recent: string[];
    onPick: (folder: string) => void;
    onClose: () => void;
  }

  let { title, pinned, library, recent, onPick, onClose }: Props = $props();

  let typed = $state("");
  let current = $state(0);
  let field = $state<HTMLInputElement>();

  const shown = $derived(destinations(pinned, library, recent, typed));
  const KIND_WORDS: Record<Destination["kind"], string> = { pinned: "Pinned", library: "Search Folder", recent: "Recent" };

  async function chooseOther() {
    const folder = await chooseFolder();
    if (folder) onPick(folder);
  }

  function onKeyDown(event: KeyboardEvent) {
    if (event.key === "ArrowDown" || event.key === "ArrowUp") {
      event.preventDefault();
      current = Math.max(0, Math.min(shown.length, current + (event.key === "ArrowDown" ? 1 : -1)));
    } else if (event.key === "Enter") {
      event.preventDefault();
      if (current < shown.length) onPick(shown[current].path);
      else chooseOther();
    } else if (event.key === "Escape") {
      event.preventDefault();
      event.stopPropagation();
      onClose();
    }
  }

  onMount(() => field?.focus());
</script>

<div class="glass picker" role="dialog" aria-label={title}>
  <h2>{title}</h2>
  <input bind:this={field} bind:value={typed} oninput={() => (current = 0)} onkeydown={onKeyDown} placeholder="Type to filter" aria-label="Folder name" aria-controls="folder-list" />
  <ul id="folder-list" role="listbox" aria-label="Folders">
    {#each shown as destination, i (destination.path)}
      <li role="option" aria-selected={i === current} class:current={i === current}>
        <button tabindex="-1" title={destination.path} onclick={() => onPick(destination.path)}>
          <span>{nameOf(destination.path)}</span><small>{KIND_WORDS[destination.kind]}</small>
        </button>
      </li>
    {/each}
    <li role="option" aria-selected={current === shown.length} class:current={current === shown.length}>
      <button tabindex="-1" onclick={chooseOther}><span>Choose Another Folder…</span></button>
    </li>
  </ul>
</div>

<style>
  .picker {
    position: absolute;
    top: 4rem;
    left: 50%;
    z-index: 5;
    width: min(22rem, calc(100% - 2rem));
    padding: 0.75rem;
    border-radius: var(--radius-group);
    transform: translateX(-50%);
  }

  h2 {
    margin: 0 0 0.5rem;
    font: var(--headline);
  }

  input {
    width: 100%;
    height: 2rem;
    padding: 0 0.75rem;
    border: 0;
    border-radius: 0.875rem;
    outline: 0;
    background: var(--control);
    color: var(--label);
    font: var(--body);
  }

  ul {
    max-height: 16rem;
    margin: 0.5rem 0 0;
    padding: 0;
    overflow-y: auto;
    list-style: none;
  }

  li.current {
    border-radius: var(--radius-small);
    background: var(--accent-soft);
  }

  button {
    display: flex;
    width: 100%;
    justify-content: space-between;
    padding: 0.375rem 0.5rem;
    border: 0;
    background: transparent;
    color: var(--label);
    font: var(--body);
    text-align: left;
  }

  small {
    color: var(--label-secondary);
    font: var(--subheadline);
  }
</style>

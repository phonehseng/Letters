<script lang="ts">
  // the sheet tabs along the bottom, as in excel and google sheets: a
  // click shows a sheet, a double-click renames it in place, right-click
  // (or the menu key) offers rename, duplicate, move and delete, and the
  // plus button adds a sheet after the one shown.
  import ContextMenu, { type ContextItem } from "../ContextMenu.svelte";
  import Icon from "../Icon.svelte";
  import { tooltipFor } from "../tools";
  import type { SheetTab } from "./sheetsApi";

  interface Props {
    tabs: SheetTab[];
    active: number;
    onShow: (index: number) => void;
    onAdd: () => void;
    onRename: (index: number, name: string) => void;
    onMove: (from: number, to: number) => void;
    onDuplicate: (index: number) => void;
    onDelete: (index: number) => void;
  }

  let { tabs, active, onShow, onAdd, onRename, onMove, onDuplicate, onDelete }: Props = $props();
  let renaming = $state<number | null>(null);
  let newName = $state("");
  let menu = $state<{ x: number; y: number; index: number } | null>(null);

  function startRename(index: number) {
    renaming = index;
    newName = tabs[index].name;
  }

  function finishRename(keep: boolean) {
    const index = renaming;
    renaming = null;
    if (keep && index !== null && newName.trim() && newName !== tabs[index].name) onRename(index, newName.trim());
  }

  function menuItems(index: number): ContextItem[] {
    return [
      { label: "Rename", run: () => startRename(index) },
      { label: "Duplicate", run: () => onDuplicate(index) },
      { label: "Move Left", disabled: index === 0, run: () => onMove(index, index - 1) },
      { label: "Move Right", disabled: index === tabs.length - 1, run: () => onMove(index, index + 1) },
      { label: "Delete…", separatorBefore: true, disabled: tabs.length <= 1, title: "Removes the sheet and its cells. Undo (Ctrl Z) brings it back.", run: () => onDelete(index) },
    ];
  }

  function onTabKey(event: KeyboardEvent, index: number) {
    if (event.key === "F2") startRename(index);
    else if (event.key === "ContextMenu" || (event.shiftKey && event.key === "F10")) {
      const box = (event.currentTarget as HTMLElement).getBoundingClientRect();
      menu = { x: box.left, y: box.top - 4, index };
      event.preventDefault();
    }
  }
</script>

<div class="tabs" role="tablist" aria-label="Sheets" title={tooltipFor("sheetsTabs")}>
  <button class="add" title="Add a sheet (Shift F11)" aria-label="Add Sheet" onclick={onAdd}><Icon name="plus" size={0.875} /></button>
  {#each tabs as tab, index (index)}
    {#if !tab.hidden}
      {#if renaming === index}
        <!-- svelte-ignore a11y_autofocus -->
        <input
          class="rename"
          aria-label="Sheet name"
          bind:value={newName}
          autofocus
          onblur={() => finishRename(true)}
          onkeydown={(event) => {
            if (event.key === "Enter") finishRename(true);
            if (event.key === "Escape") finishRename(false);
            event.stopPropagation();
          }}
        />
      {:else}
        <button
          class="tab"
          class:active={index === active}
          role="tab"
          aria-selected={index === active}
          style:--tab-colour={tab.color ?? "transparent"}
          onclick={() => onShow(index)}
          ondblclick={() => startRename(index)}
          oncontextmenu={(event) => {
            event.preventDefault();
            menu = { x: event.clientX, y: event.clientY, index };
          }}
          onkeydown={(event) => onTabKey(event, index)}>{tab.name}</button
        >
      {/if}
    {/if}
  {/each}
</div>

{#if menu}
  <ContextMenu x={menu.x} y={menu.y} label={`Sheet ${tabs[menu.index]?.name ?? ""}`} items={menuItems(menu.index)} onClose={() => (menu = null)} />
{/if}

<style>
  .tabs {
    display: flex;
    align-items: stretch;
    gap: 0.125rem;
    min-width: 0;
    overflow-x: auto;
    scrollbar-width: none;
  }

  .add,
  .tab {
    flex: none;
    height: 1.75rem;
    padding: 0 0.75rem;
    border: 0;
    border-radius: var(--radius-control);
    background: transparent;
    color: var(--label-secondary);
    font: var(--body);
    white-space: nowrap;
  }

  .add {
    display: grid;
    place-items: center;
    padding: 0 0.5rem;
  }

  .tab {
    box-shadow: inset 0 -3px 0 var(--tab-colour);
  }

  .add:hover,
  .tab:hover {
    background: var(--control-hover);
  }

  .tab.active {
    background: var(--segment-selected);
    color: var(--label);
    font-weight: 600;
    box-shadow:
      var(--shadow-segment),
      inset 0 -3px 0 var(--tab-colour);
  }

  .rename {
    width: 8rem;
    height: 1.75rem;
    padding: 0 0.5rem;
    border: 0;
    border-radius: var(--radius-control);
    outline: 2px solid var(--accent);
    font: var(--body);
  }
</style>

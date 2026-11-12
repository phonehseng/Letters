<script lang="ts">
  // a small sheet under the toolbar that asks for one thing, such as a
  // link's address or a comment's words. Enter confirms, Escape cancels,
  // and the focus goes back to the document either way.
  import { onMount } from "svelte";

  interface Props {
    title: string;
    label: string;
    value?: string;
    placeholder?: string;
    confirm: string;
    top: number;
    /** a second, quieter action (Remove Link) */
    secondary?: { label: string; run: () => void };
    multiline?: boolean;
    onSubmit: (value: string) => void;
    onCancel: () => void;
  }

  let { title, label, value = "", placeholder = "", confirm, top, secondary, multiline = false, onSubmit, onCancel }: Props = $props();
  let text = $state("");
  let field = $state<HTMLInputElement | HTMLTextAreaElement>();

  onMount(() => {
    text = value;
    field?.focus();
    field?.select();
  });

  function onKey(event: KeyboardEvent) {
    if (event.key === "Escape") {
      event.preventDefault();
      event.stopPropagation();
      onCancel();
    } else if (event.key === "Enter" && (!multiline || event.ctrlKey || event.metaKey)) {
      event.preventDefault();
      if (text.trim()) onSubmit(text.trim());
    }
  }
</script>

<div class="glass sheet" role="dialog" aria-label={title} style:top="{top + 8}px">
  <h2>{title}</h2>
  <label class="setting">
    <span class="caption">{label}</span>
    {#if multiline}
      <textarea class="field" rows="3" {placeholder} bind:value={text} bind:this={field} onkeydown={onKey}></textarea>
    {:else}
      <input class="field" type="text" {placeholder} bind:value={text} bind:this={field} onkeydown={onKey} />
    {/if}
  </label>
  <div class="buttons">
    {#if secondary}<button class="button" onclick={secondary.run}>{secondary.label}</button>{/if}
    <span class="spacer"></span>
    <button class="button" onclick={onCancel}>Cancel</button>
    <button class="button primary" disabled={!text.trim()} onclick={() => onSubmit(text.trim())}>{confirm}</button>
  </div>
</div>

<style>
  .sheet {
    position: absolute;
    left: 50%;
    z-index: 4;
    display: flex;
    flex-direction: column;
    gap: 0.625rem;
    width: min(24rem, calc(100% - 2rem));
    padding: 0.875rem 1rem;
    transform: translateX(-50%);
  }

  h2 {
    margin: 0;
    font: var(--headline);
  }

  .buttons {
    display: flex;
    gap: 0.5rem;
  }

  .spacer {
    flex: 1;
  }

  textarea {
    resize: vertical;
    font: var(--body);
  }
</style>

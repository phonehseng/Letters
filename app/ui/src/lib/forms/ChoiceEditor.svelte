<script lang="ts">
  // choosing from a pop-up menu (dropdown) or a list, right on the page.
  //
  // it is the system's own menu control placed over the field, so it
  // opens, scrolls and types-to-find the way every other menu does; a
  // click on the field opens the menu at once. a pop-up menu that also
  // takes typed answers is a text field with the choices as suggestions.
  // each choice is stored straight away, as one undo step.
  import { onMount, tick, untrack } from "svelte";
  import type { PageFrame } from "../pageTools/pagePoints";
  import { editorFontSize } from "./fieldEditing";
  import type { FieldInfo } from "./formsApi";
  import type { FillSignSession } from "./fillSignSession.svelte";
  import { fieldLabel } from "./tabOrder";

  interface Props {
    session: FillSignSession;
    field: FieldInfo;
    widget: number;
    frame: PageFrame;
    onFocusDocument: () => void;
  }

  let { session, field, widget, frame, onFocusDocument }: Props = $props();

  const rect = $derived(field.widgets[widget].rect);
  const fontSize = $derived(Math.min(editorFontSize(field, rect), 14));
  const listId = `choices-${Math.random().toString(36).slice(2)}`;
  let control = $state<HTMLSelectElement | HTMLInputElement>();
  let typed = $state(untrack(() => field.value ?? ""));

  onMount(() => {
    tick().then(() => {
      control?.focus({ preventScroll: true });
      // opened from a click: show the choices at once, as a menu would
      if (control instanceof HTMLSelectElement && field.kind === "dropdown" && session.lastPress === "pointer") {
        try {
          control.showPicker();
        } catch {
          // older engines open it on the next click instead
        }
      }
    });
  });

  function chosen(): string[] {
    if (!(control instanceof HTMLSelectElement)) return typed ? [typed] : [];
    return [...control.selectedOptions].map((option) => option.value);
  }

  function onChange() {
    session.choose(field.name, chosen());
  }

  function onKeyDown(event: KeyboardEvent) {
    if (event.key === "Tab") {
      event.preventDefault();
      if (!(control instanceof HTMLSelectElement)) onChange();
      session.tab(field.name, event.shiftKey);
    } else if (event.key === "Escape") {
      event.preventDefault();
      event.stopPropagation();
      session.leave();
      onFocusDocument();
    } else if (event.key === "Enter" && !(control instanceof HTMLSelectElement)) {
      event.preventDefault();
      onChange();
    }
  }

  function onFocusOut(event: FocusEvent) {
    const next = event.relatedTarget as HTMLElement | null;
    if (!(control instanceof HTMLSelectElement)) onChange();
    if (!next?.closest("[data-forms-editor]") && session.active?.field === field.name) session.leave();
  }
</script>

<div
  class="editor-area"
  data-page-ui
  data-forms-editor
  style:left="{rect.x * frame.scale}px"
  style:top="{rect.y * frame.scale}px"
  style:width="{rect.width * frame.scale}px"
  style:height="{rect.height * frame.scale}px"
>
  {#if field.kind === "dropdown" && field.editable}
    <input
      bind:this={control}
      bind:value={typed}
      class="editor"
      list={listId}
      aria-label={fieldLabel(field)}
      aria-required={field.required || undefined}
      style:font-size="{fontSize * frame.scale}px"
      onkeydown={onKeyDown}
      onfocusout={onFocusOut}
    />
    <datalist id={listId}>
      {#each field.options as option (option.value)}<option value={option.value}>{option.label}</option>{/each}
    </datalist>
  {:else}
    <select
      bind:this={control}
      class="editor"
      class:list={field.kind === "list"}
      multiple={field.multiSelect}
      size={field.kind === "list" ? Math.max(2, Math.min(field.options.length, Math.floor(rect.height / 14))) : undefined}
      aria-label={fieldLabel(field)}
      aria-required={field.required || undefined}
      style:font-size="{fontSize * frame.scale}px"
      onchange={onChange}
      onkeydown={onKeyDown}
      onfocusout={onFocusOut}
    >
      {#if field.kind === "dropdown" && field.selected.length === 0}
        <option value="" disabled selected>Choose…</option>
      {/if}
      {#each field.options as option (option.value)}
        <option value={option.value} selected={field.selected.includes(option.value)}>{option.label}</option>
      {/each}
    </select>
  {/if}
</div>

<style>
  .editor-area {
    position: absolute;
    z-index: 2;
    pointer-events: auto;
  }

  .editor {
    box-sizing: border-box;
    width: 100%;
    height: 100%;
    margin: 0;
    padding: 0 2px;
    border: 0;
    border-radius: 1px;
    outline: 1px solid var(--accent);
    background: #ffffff;
    color: #000000;
    font-family: Helvetica, Arial, sans-serif;
  }

  .editor.list {
    padding: 0;
  }

  .editor:focus-visible {
    outline: 1px solid var(--accent);
    box-shadow: 0 0 0 3px var(--accent-soft);
  }
</style>

<script lang="ts">
  // typing into a text, date or comb field right on the page.
  //
  // the editor sits exactly over the field, at about the size the engine
  // will draw the text, so what is typed looks like the answer. typing
  // stays here until the field is left (Tab, Enter, a click elsewhere,
  // Ctrl S), and then the whole answer is one change and one undo step.
  // Escape puts the answer back as it was. a date field also has a small
  // calendar button; a comb field puts one letter in each box.
  import { onDestroy, onMount, tick, untrack } from "svelte";
  import type { PageFrame } from "../pageTools/pagePoints";
  import { combSpacing, dateHint, editorFontSize, editorKind, isoDate, startingText, withinLimit } from "./fieldEditing";
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
  const kind = $derived(editorKind(field));
  const fontSize = $derived(editorFontSize(field, rect));
  const comb = $derived(kind === "comb" && field.maxLength ? combSpacing(rect, field.maxLength, fontSize) : null);
  const hintId = `field-hint-${Math.random().toString(36).slice(2)}`;

  // the editor starts with the field's answer; a new editor is made for another field
  let text = $state(untrack(() => startingText(field)));
  // what was last handed to the engine, so leaving twice doesn't send it twice
  let sent = untrack(() => startingText(field));
  let input = $state<HTMLInputElement | HTMLTextAreaElement>();
  let picker = $state<HTMLInputElement>();
  let cancelled = false;

  onMount(() => {
    tick().then(() => {
      input?.focus({ preventScroll: true });
      input?.select();
    });
  });

  // a change that comes back from the engine (undo, another window) shows here too
  $effect(() => {
    const now = startingText(field);
    untrack(() => {
      if (now !== sent && document.activeElement !== input) {
        text = now;
        sent = now;
      }
    });
  });

  function commit() {
    if (cancelled || text === sent) return;
    sent = text;
    session.commitText(field.name, text);
  }

  // a click on another field takes this editor away without a blur, so
  // the answer is stored as it goes
  onDestroy(commit);

  function onKeyDown(event: KeyboardEvent) {
    const multiline = kind === "multiline";
    if (event.key === "Tab") {
      event.preventDefault();
      commit();
      session.tab(field.name, event.shiftKey);
    } else if (event.key === "Enter" && (!multiline || event.ctrlKey || event.metaKey)) {
      event.preventDefault();
      commit();
    } else if (event.key === "Escape") {
      event.preventDefault();
      event.stopPropagation();
      cancelled = true;
      text = sent;
      session.leave();
      onFocusDocument();
    } else if ((event.ctrlKey || event.metaKey) && event.key.toLowerCase() === "s") {
      // saving while typing: the answer goes first, so the save includes it
      commit();
    }
  }

  function onInput() {
    const limited = withinLimit(text, field.maxLength);
    if (limited !== text) text = limited;
  }

  /** leaving the field stores the answer; the field lets go unless the focus stayed in it (the calendar). */
  function onFocusOut(event: FocusEvent) {
    const next = event.relatedTarget as HTMLElement | null;
    if (next && next === picker) return;
    commit();
    if (!next?.closest("[data-forms-editor]") && session.active?.field === field.name) session.leave();
  }

  function openCalendar() {
    try {
      picker?.showPicker();
    } catch {
      picker?.focus();
    }
  }

  function onPicked() {
    if (!picker?.value) return;
    const [year, month, day] = picker.value.split("-").map(Number);
    text = isoDate(new Date(year, month - 1, day));
    commit();
    input?.focus();
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
  {#if kind === "multiline"}
    <textarea
      bind:this={input}
      bind:value={text}
      class="editor"
      aria-label={fieldLabel(field)}
      aria-required={field.required || undefined}
      aria-describedby={hintId}
      spellcheck="true"
      style:font-size="{fontSize * frame.scale}px"
      style:text-align={field.alignment}
      oninput={onInput}
      onkeydown={onKeyDown}
      onfocusout={onFocusOut}
    ></textarea>
  {:else}
    <input
      bind:this={input}
      bind:value={text}
      class="editor"
      class:comb={comb !== null}
      type={field.password ? "password" : "text"}
      maxlength={field.maxLength ?? undefined}
      aria-label={fieldLabel(field)}
      aria-required={field.required || undefined}
      aria-describedby={hintId}
      spellcheck={kind === "text"}
      autocomplete="off"
      style:font-size="{fontSize * frame.scale}px"
      style:text-align={comb ? "left" : field.alignment}
      style:letter-spacing={comb ? `${comb.letterSpacing * frame.scale}px` : undefined}
      style:padding-left={comb ? `${comb.paddingLeft * frame.scale}px` : undefined}
      oninput={onInput}
      onkeydown={onKeyDown}
      onfocusout={onFocusOut}
    />
  {/if}
  {#if kind === "date"}
    <button class="calendar" type="button" tabindex="-1" title="Choose a date" aria-label="Choose a date" onclick={openCalendar}>
      <svg viewBox="0 0 16 16" aria-hidden="true"
        ><rect x="2" y="3.5" width="12" height="10.5" rx="1.5" /><path d="M2 7h12M5.5 2v3M10.5 2v3" /></svg
      >
    </button>
    <input bind:this={picker} class="picker" type="date" tabindex="-1" aria-hidden="true" onchange={onPicked} />
  {/if}
  <span id={hintId} class="visually-hidden">
    {kind === "date" ? dateHint(field) : ""}
    {field.maxLength ? `At most ${field.maxLength} characters.` : ""}
    Tab goes to the next field. Escape cancels.
  </span>
</div>

<style>
  .editor-area {
    position: absolute;
    z-index: 2;
    pointer-events: auto;
  }

  /* nearly white, so the answer the engine drew before doesn't show through while typing */
  .editor {
    box-sizing: border-box;
    width: 100%;
    height: 100%;
    margin: 0;
    padding: 0 2px;
    border: 0;
    border-radius: 1px;
    outline: 1px solid var(--accent);
    background: rgb(255 255 255 / 0.96);
    color: #000000;
    font-family: Helvetica, Arial, sans-serif;
    line-height: 1.15;
    resize: none;
    letter-spacing: 0;
  }

  textarea.editor {
    padding: 2px;
    overflow: auto;
  }

  .editor.comb {
    font-family: "Courier New", Courier, monospace;
  }

  .editor:focus-visible {
    outline: 1px solid var(--accent);
    box-shadow: 0 0 0 3px var(--accent-soft);
  }

  .calendar {
    position: absolute;
    top: 50%;
    right: 2px;
    display: grid;
    place-items: center;
    width: 1.125rem;
    height: 1.125rem;
    padding: 0;
    border: 0;
    border-radius: 0.25rem;
    background: var(--control);
    transform: translateY(-50%);
  }

  .calendar svg {
    width: 0.75rem;
    height: 0.75rem;
    fill: none;
    stroke: #333333;
    stroke-width: 1.4;
    stroke-linecap: round;
  }

  /* the browser's own date picker, opened by the calendar button */
  .picker {
    position: absolute;
    right: 0;
    bottom: 0;
    width: 1px;
    height: 1px;
    opacity: 0;
    pointer-events: none;
  }
</style>

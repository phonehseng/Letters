<script lang="ts">
  // one place of a form field on the page, while filling.
  //
  // it is a soft tinted box over the field the engine drew. a text, date
  // or choice field opens its editor right there when it is clicked or
  // reached with Tab; a checkbox or radio button changes with a click or
  // Space. Tab and Shift Tab follow the form's order across pages (see
  // tabOrder.ts), and the arrows move between the buttons of a radio
  // group. a required field without an answer carries a small mark, and
  // the engine's refusal shows beside the field in plain words.
  import { tick } from "svelte";
  import type { PageFrame } from "../pageTools/pagePoints";
  import ChoiceEditor from "./ChoiceEditor.svelte";
  import { editorKind, neighbourWidget, widgetIsOn } from "./fieldEditing";
  import type { FieldInfo } from "./formsApi";
  import type { FillSignSession } from "./fillSignSession.svelte";
  import { fieldLabel, isFilled } from "./tabOrder";
  import TextFieldEditor from "./TextFieldEditor.svelte";

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
  const isActive = $derived(session.active?.field === field.name && session.active.widget === widget);
  const editing = $derived(isActive && kind !== "toggle" && kind !== "none");
  const on = $derived(widgetIsOn(field, widget));
  const error = $derived(session.errors[field.name] ?? null);
  const missing = $derived(field.required && !isFilled(field));
  const label = $derived(fieldLabel(field));
  const signature = $derived(field.kind === "signature");
  // a radio group is one Tab stop: its chosen button, or its first
  const tabStop = $derived(field.kind !== "radioGroup" || on || (field.value === null && widget === 0));
  let element = $state<HTMLElement>();

  // the session asks for the focus when Tab, Next Empty Field or the field list goes here
  $effect(() => {
    void session.focusTick;
    if (isActive && !editing) tick().then(() => element?.focus({ preventScroll: true }));
  });

  function activate(how: "pointer" | "keyboard" = "keyboard") {
    if (field.readOnly) return;
    if (signature) {
      if (!field.signed) session.requestSignature(field.name);
      return;
    }
    session.focusField(field.name, widget, how);
  }

  function onKeyDown(event: KeyboardEvent) {
    if (event.key === "Tab") {
      event.preventDefault();
      session.tab(field.name, event.shiftKey);
    } else if (event.key === "Escape") {
      event.preventDefault();
      event.stopPropagation();
      session.leave();
      onFocusDocument();
    } else if (kind === "toggle" && (event.key === " " || event.key === "Enter")) {
      event.preventDefault();
      session.toggle(field.name, widget);
    } else if (kind === "toggle") {
      const next = neighbourWidget(field, widget, event.key);
      if (next === null) return;
      // arrows choose the next button of the group, as radio buttons do everywhere
      event.preventDefault();
      session.toggle(field.name, next);
      session.focusField(field.name, next);
    } else if (signature && (event.key === "Enter" || event.key === " ")) {
      event.preventDefault();
      activate();
    } else if (event.key === "Enter" || event.key === "F2") {
      event.preventDefault();
      activate();
    }
  }

  function role(): string {
    if (field.kind === "checkbox") return "checkbox";
    if (field.kind === "radioGroup") return "radio";
    if (signature) return "button";
    return "textbox";
  }

  function spokenName(): string {
    if (signature) return field.signed ? `${label}, signed` : `${label}, signature field. Press Enter to sign.`;
    return label;
  }
</script>

{#if editing}
  {#if kind === "dropdown" || kind === "list"}
    <ChoiceEditor {session} {field} {widget} {frame} {onFocusDocument} />
  {:else}
    <TextFieldEditor {session} {field} {widget} {frame} {onFocusDocument} />
  {/if}
{:else}
  <!-- svelte-ignore a11y_no_noninteractive_tabindex, a11y_interactive_supports_focus -->
  <div
    bind:this={element}
    class="box"
    class:read-only={field.readOnly}
    class:signature
    class:signed={field.signed}
    class:round={field.kind === "radioGroup"}
    class:missing
    role={role()}
    tabindex={field.readOnly || !tabStop ? -1 : 0}
    aria-label={spokenName()}
    aria-checked={kind === "toggle" ? on : undefined}
    aria-required={field.required || undefined}
    aria-readonly={field.readOnly || undefined}
    aria-invalid={error ? true : undefined}
    title={field.tooltip ?? undefined}
    style:left="{rect.x * frame.scale}px"
    style:top="{rect.y * frame.scale}px"
    style:width="{rect.width * frame.scale}px"
    style:height="{rect.height * frame.scale}px"
    onpointerdown={(event) => {
      // the press is the field's, not the page tool's or the text selection's
      event.preventDefault();
    }}
    onclick={(event) => {
      const how = event.detail === 0 ? "keyboard" : "pointer";
      if (kind === "toggle") {
        session.toggle(field.name, widget);
        session.focusField(field.name, widget, how);
      } else activate(how);
    }}
    onfocus={() => {
      // reached with the browser's own Tab: become the active field (and open its editor)
      if (!isActive && !field.readOnly && !signature) session.focusField(field.name, widget);
    }}
    onkeydown={onKeyDown}
  >
    {#if signature && !field.signed}<span class="sign-here">Sign Here</span>{/if}
  </div>
{/if}

{#if missing && !field.readOnly}
  <span
    class="required-mark"
    aria-hidden="true"
    title="Required"
    style:left="{(rect.x + rect.width) * frame.scale + 2}px"
    style:top="{rect.y * frame.scale - 4}px">*</span
  >
{/if}

{#if error}
  <p class="glass error" style:left="{rect.x * frame.scale}px" style:top="{(rect.y + rect.height) * frame.scale + 4}px">{error}</p>
{/if}

<style>
  /* the soft tint of a form field: a light blue that reads as "fill me in",
     lighter than the accent, which stays for what is selected */
  .box {
    position: absolute;
    box-sizing: border-box;
    pointer-events: auto;
    border: 1px solid rgb(52 120 246 / 0.32);
    border-radius: 2px;
    background: rgb(52 120 246 / 0.1);
    cursor: text;
    transition: background 0.15s var(--ease);
  }

  .box[role="checkbox"],
  .box[role="radio"],
  .box.signature {
    cursor: default;
  }

  .box.round {
    border-radius: 50%;
  }

  .box:hover {
    background: rgb(52 120 246 / 0.16);
  }

  .box.read-only {
    border-color: rgb(0 0 0 / 0.12);
    background: rgb(0 0 0 / 0.04);
    cursor: not-allowed;
  }

  .box.signature {
    display: grid;
    place-items: center;
    border-style: dashed;
  }

  .box.signed {
    border-style: solid;
    border-color: rgb(40 150 85 / 0.5);
    background: rgb(40 150 85 / 0.06);
  }

  .sign-here {
    color: rgb(30 80 190);
    font: var(--footnote);
  }

  .required-mark {
    position: absolute;
    color: var(--danger);
    font: var(--headline);
    pointer-events: none;
  }

  .error {
    position: absolute;
    z-index: 2;
    max-width: 16rem;
    margin: 0;
    padding: 0.25rem 0.5rem;
    border-radius: var(--radius-small);
    color: var(--label);
    font: var(--subheadline);
    letter-spacing: var(--subheadline-spacing);
    pointer-events: none;
    box-shadow:
      inset 3px 0 0 var(--danger),
      var(--shadow-glass);
  }

  @media (prefers-reduced-transparency: reduce) {
    .error {
      background: var(--window);
    }
  }
</style>

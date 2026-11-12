<script lang="ts">
  // the settings of the field selected in Edit Fields: its name, the help
  // text readers see (and screen readers say), whether it is required,
  // the text's alignment and size, and the choices of a menu or list.
  // each setting is changed when the person leaves it, as one undo step.
  import { untrack } from "svelte";
  import type { Alignment, FieldChanges, FieldInfo } from "../../forms/formsApi";
  import type { FillSignSession } from "../../forms/fillSignSession.svelte";
  import { parseChoices } from "../../forms/newFields";
  import Choices from "../Choices.svelte";

  interface Props {
    session: FillSignSession;
    field: FieldInfo;
  }

  let { session, field }: Props = $props();

  const takesText = $derived(field.kind === "text" || field.kind === "multilineText" || field.kind === "date");
  const hasChoices = $derived(field.kind === "dropdown" || field.kind === "list");

  let name = $state(untrack(() => field.name));
  let tooltip = $state(untrack(() => field.tooltip ?? ""));
  let fontSize = $state(untrack(() => field.fontSize));
  let alignment = $state<Alignment>(untrack(() => field.alignment));
  let choices = $state(untrack(() => field.options.map((option) => option.label).join("\n")));

  function change(changes: FieldChanges) {
    session.changeField(field.name, changes);
  }

  function rename() {
    const typed = name.trim();
    if (typed && typed !== field.name) change({ newName: typed });
    else name = field.name;
  }

  // the alignment control changes at once, like the other segmented controls
  let lastAlignment = untrack(() => field.alignment);
  $effect(() => {
    const chosen = alignment;
    untrack(() => {
      if (chosen !== lastAlignment) {
        lastAlignment = chosen;
        change({ alignment: chosen });
      }
    });
  });
</script>

<div class="properties">
  <label class="setting">
    <span class="caption">Name</span>
    <!-- change fires on Enter and when the field is left, once each time the name changed -->
    <input class="field" bind:value={name} onchange={rename} />
  </label>
  <label class="setting">
    <span class="caption">Help Text</span>
    <input
      class="field"
      placeholder="Shown when the pointer rests on the field"
      bind:value={tooltip}
      onchange={() => tooltip !== (field.tooltip ?? "") && change({ tooltip })}
    />
  </label>
  {#if field.kind !== "signature"}
    <label class="check">
      <input type="checkbox" checked={field.required} onchange={(event) => change({ required: event.currentTarget.checked })} />
      Required
    </label>
  {/if}
  {#if takesText}
    <Choices
      label="Alignment"
      options={[
        { value: "left", label: "Left" },
        { value: "center", label: "Centre" },
        { value: "right", label: "Right" },
      ]}
      bind:value={alignment}
    />
    <label class="setting">
      <span class="caption">Text Size (0 fits the field)</span>
      <input
        class="field"
        type="number"
        min="0"
        max="72"
        step="1"
        bind:value={fontSize}
        onchange={() => Number.isFinite(fontSize) && fontSize !== field.fontSize && change({ fontSize: Math.max(0, fontSize) })}
      />
    </label>
  {/if}
  {#if hasChoices}
    <label class="setting">
      <span class="caption">Choices, one per line</span>
      <textarea
        class="field"
        rows="4"
        bind:value={choices}
        onchange={() => {
          const options = parseChoices(choices);
          if (options.length > 0) change({ options });
        }}
      ></textarea>
    </label>
  {/if}
  <div class="actions">
    <button class="button" onclick={() => session.deleteField(field.name)}>Delete Field</button>
  </div>
</div>

<style>
  .properties {
    display: flex;
    flex-direction: column;
    gap: 0.625rem;
  }

  .actions {
    display: flex;
    justify-content: flex-start;
  }
</style>

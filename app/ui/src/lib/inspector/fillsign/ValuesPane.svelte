<script lang="ts">
  // Prepare Form > Values: what a checkbox or radio button stands for when
  // chosen (its export value), the answer a field starts with (its
  // default), clearing the form back to those defaults, and adding reset
  // and print buttons. there is no submit button: the engine's sentence
  // says why (sending answers needs a server, and Letters keeps
  // everything on this computer).
  import { buttonPlace, resetEffect } from "../../forms/formBuilding";
  import { addButton, resetForm, setDefaultValue, setExportValue } from "../../forms/formBuildingApi";
  import type { FieldValue } from "../../forms/formsApi";
  import { fieldLabel } from "../../forms/tabOrder";
  import type { ToolContext } from "../../inspector";
  import Choices from "../Choices.svelte";

  interface Props {
    context: ToolContext;
    noSubmitNote: string | null;
  }

  let { context, noSubmitNote }: Props = $props();

  const session = $derived(context.fillSign!);
  let task = $state<"export" | "default" | "reset" | "buttons">("default");
  let fieldName = $state("");
  const field = $derived(session.field(fieldName));

  // ---- export values ----
  const choosable = $derived(session.fields.filter((each) => each.kind === "checkbox" || each.kind === "radioGroup"));
  let button = $state(0);
  let exportValue = $state("");

  async function saveExportValue() {
    const id = context.document.id;
    const name = fieldName;
    const value = exportValue.trim();
    const worked = await context.change(() => setExportValue(id, name, button, value));
    if (worked) context.notify(`When chosen, it now stands for “${value}”.`);
  }

  // ---- defaults ----
  const answerable = $derived(session.fields.filter((each) => each.kind !== "signature" && each.kind !== "button"));
  let defaultText = $state("");
  let defaultChecked = $state(false);

  function defaultAnswer(): FieldValue | null {
    if (field?.kind === "checkbox") return defaultChecked ? { kind: "checked", value: true } : null;
    return defaultText.trim() ? { kind: "text", value: defaultText } : null;
  }

  async function saveDefault() {
    const id = context.document.id;
    const name = fieldName;
    const answer = defaultAnswer();
    const worked = await context.change(() => setDefaultValue(id, name, answer));
    if (worked) context.notify(answer ? "Default answer set." : "Default answer removed.");
  }

  // ---- reset and buttons ----
  async function reset() {
    const id = context.document.id;
    const only = fieldName ? [fieldName] : null;
    let count = 0;
    const worked = await context.change(async () => {
      const edit = await resetForm(id, only);
      count = edit.names.length;
      return edit.document;
    });
    if (worked) context.notify(`${count} ${count === 1 ? "field" : "fields"} put back to the default answer.`);
  }

  let resetButton = $state(true);
  let printButton = $state(false);

  async function addButtons() {
    const id = context.document.id;
    const page = context.currentPage;
    const size = context.document.pages[page];
    if (!size) return;
    const wanted = [
      resetButton && { name: "Reset", label: "Clear Form", action: { type: "resetForm" as const } },
      printButton && { name: "Print", label: "Print", action: { type: "print" as const } },
    ].filter((each) => each !== false);
    const worked = await context.change(async () => {
      let last = null;
      for (const [index, each] of wanted.entries()) last = await addButton(id, { ...each, page, rect: buttonPlace(size, index) });
      return last!.document;
    });
    if (worked) context.notify(`Added to the bottom of page ${page + 1}. Use Edit Fields to move them.`);
  }
</script>

<Choices
  label="Values"
  hideLabel
  options={[
    { value: "default", label: "Default" },
    { value: "export", label: "Stands For" },
    { value: "reset", label: "Reset" },
    { value: "buttons", label: "Buttons" },
  ]}
  bind:value={task}
/>

{#if task === "export"}
  <label class="setting">
    <span class="caption">Checkbox or Radio Group</span>
    <select class="field" bind:value={fieldName}>
      <option value="">Choose a field</option>
      {#each choosable as each (each.name)}<option value={each.name}>{fieldLabel(each)}</option>{/each}
    </select>
  </label>
  {#if field && field.widgets.length > 1}
    <label class="setting">
      <span class="caption">Button</span>
      <select class="field" bind:value={button}>
        {#each field.widgets as widget, index (index)}<option value={index}>{widget.onValue ?? `Button ${index + 1}`}</option>{/each}
      </select>
    </label>
  {/if}
  <label class="setting">
    <span class="caption">Stands For</span>
    <input class="field" placeholder="Yes" bind:value={exportValue} />
    <span class="hint">The value saved and exported when it is chosen, such as Yes or Vegetarian.</span>
  </label>
  <div class="actions"><button class="button primary" disabled={!field || !exportValue.trim()} onclick={saveExportValue}>Set Value</button></div>
{:else if task === "default"}
  <label class="setting">
    <span class="caption">Field</span>
    <select class="field" bind:value={fieldName}>
      <option value="">Choose a field</option>
      {#each answerable as each (each.name)}<option value={each.name}>{fieldLabel(each)}</option>{/each}
    </select>
  </label>
  {#if field?.kind === "checkbox"}
    <label class="check"><input type="checkbox" bind:checked={defaultChecked} /> Ticked to start with</label>
  {:else}
    <label class="setting">
      <span class="caption">Starts With</span>
      <input class="field" placeholder="Leave empty for none" bind:value={defaultText} />
    </label>
  {/if}
  <div class="actions"><button class="button primary" disabled={!field} onclick={saveDefault}>Set Default</button></div>
  <p class="effect">The answer the field goes back to when the form is reset. It doesn’t fill the field now.</p>
{:else if task === "reset"}
  <label class="setting">
    <span class="caption">Fields</span>
    <select class="field" bind:value={fieldName}>
      <option value="">Every field</option>
      {#each answerable as each (each.name)}<option value={each.name}>{fieldLabel(each)}</option>{/each}
    </select>
  </label>
  <div class="actions"><button class="button primary" onclick={reset}>Reset</button></div>
  <p class="effect" aria-live="polite">{resetEffect(field)}</p>
{:else}
  <label class="check"><input type="checkbox" bind:checked={resetButton} /> Clear Form button</label>
  <label class="check"><input type="checkbox" bind:checked={printButton} /> Print button</label>
  <div class="actions">
    <button class="button primary" disabled={!resetButton && !printButton} onclick={addButtons}>Add Buttons</button>
  </div>
  <p class="effect">Adds the buttons at the bottom right of page {context.currentPage + 1}. They use the standard actions, not scripts.</p>
  {#if noSubmitNote}<p class="hint">{noSubmitNote}</p>{/if}
{/if}

<style>
  .actions {
    display: flex;
    justify-content: flex-end;
  }

  .effect {
    margin: 0;
    color: var(--label-secondary);
    font: var(--subheadline);
    line-height: 0.9375rem;
  }
</style>

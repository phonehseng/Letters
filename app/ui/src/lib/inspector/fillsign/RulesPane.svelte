<script lang="ts">
  // Prepare Form > Rules: one field's calculation, format and checks.
  // the draft lives here (forms/formRules.ts turns it into the engine's
  // model); Set Rules writes it as acrobat's standard functions, one undo
  // step, and the engine works the form out again. the form-wide problems
  // (circles, unknown fields) and the scripts Letters leaves alone are
  // listed under it in the engine's own sentences.
  import { untrack } from "svelte";
  import { recalculate, setFieldLogic, type FormLogicView } from "../../forms/formLogicApi";
  import {
    CALCULATION_LABELS,
    changesFrom,
    draftFrom,
    FORMAT_LABELS,
    NEGATIVE_LABELS,
    ruleSummary,
    SEPARATOR_LABELS,
    SPECIAL_LABELS,
    TIME_PATTERNS,
    type CalculationChoice,
    type RuleDraft,
  } from "../../forms/formRules";
  import { fieldLabel } from "../../forms/tabOrder";
  import type { ToolContext } from "../../inspector";

  interface Props {
    context: ToolContext;
    /** the form's logic, read again after each change by the parent */
    logic: FormLogicView | null;
  }

  let { context, logic }: Props = $props();

  const session = $derived(context.fillSign!);
  // signatures and buttons take no answers, so they have no rules
  const ruleFields = $derived(session.fields.filter((field) => field.kind !== "signature" && field.kind !== "button"));
  let fieldName = $state("");
  let draft = $state<RuleDraft>(draftFrom(undefined, { required: false, maxLength: null }));
  const fieldLogic = $derived(logic?.fields.find((each) => each.field === fieldName));
  const others = $derived(ruleFields.filter((field) => field.name !== fieldName));
  const checked = $derived(changesFrom(draft));
  const problem = $derived("problem" in checked ? checked.problem : null);

  // the field selected on the page (Edit Fields), else the first, is the one shown
  $effect(() => {
    const selected = session.selectedField?.field;
    const names = ruleFields.map((field) => field.name);
    untrack(() => {
      if (selected && names.includes(selected)) fieldName = selected;
      else if (!names.includes(fieldName)) fieldName = names[0] ?? "";
    });
  });

  // a new field, or the logic read again, starts the draft over from the file
  $effect(() => {
    const name = fieldName;
    const read = fieldLogic;
    untrack(() => {
      const field = session.field(name);
      if (field) draft = draftFrom(read, field);
    });
  });

  // a form made elsewhere can arrive with totals that don't match its answers
  async function workOutAgain() {
    const id = context.document.id;
    let count = 0;
    const worked = await context.change(async () => {
      const result = await recalculate(id);
      count = result.changed.length;
      return result.document;
    });
    if (worked) context.notify(`${count} ${count === 1 ? "total was" : "totals were"} worked out again.`);
  }

  function toggleSource(name: string, on: boolean) {
    const rest = draft.calculationFields.filter((each) => each !== name);
    draft.calculationFields = on ? [...rest, name] : rest;
  }

  async function setRules() {
    if (!("changes" in checked) || !fieldName) return;
    const id = context.document.id;
    const name = fieldName;
    const changes = checked.changes;
    const worked = await context.change(async () => (await setFieldLogic(id, name, changes)).document);
    if (worked) context.notify(`Rules set for “${fieldLabel(session.field(name) ?? { name, tooltip: null })}”.`);
  }
</script>

{#if ruleFields.length === 0}
  <p class="hint">This form has no fields that take answers yet. Add Field puts one on the page.</p>
{:else}
  <label class="setting">
    <span class="caption">Field</span>
    <select class="field" bind:value={fieldName}>
      {#each ruleFields as field (field.name)}<option value={field.name}>{fieldLabel(field)}</option>{/each}
    </select>
    <span class="hint">{ruleSummary(fieldLogic)}</span>
  </label>

  <label class="setting">
    <span class="caption">Calculate</span>
    <select class="field" bind:value={draft.calculation}>
      {#each Object.entries(CALCULATION_LABELS) as [value, label] (value)}
        {#if value !== "script" || draft.calculationScript}<option {value}>{label}</option>{/if}
      {/each}
    </select>
  </label>
  {#if draft.calculation === "formula"}
    <label class="setting">
      <span class="caption">Formula</span>
      <input class="field" placeholder="price * quantity" bind:value={draft.formula} />
      <span class="hint">Field names, numbers, + − * / and brackets; 15% is 0.15. Put names with spaces in double quotes: "Line 1" * 2.</span>
    </label>
  {:else if draft.calculation === "script"}
    <p class="hint">This field is worked out by a short script Letters runs but can’t show as a formula. Choose another kind to replace it.</p>
  {:else if draft.calculation !== "none"}
    <fieldset class="setting sources">
      <legend class="caption">{CALCULATION_LABELS[draft.calculation as CalculationChoice]} of</legend>
      {#each others as field (field.name)}
        <label class="check">
          <input type="checkbox" checked={draft.calculationFields.includes(field.name)} onchange={(event) => toggleSource(field.name, event.currentTarget.checked)} />
          {fieldLabel(field)}
        </label>
      {/each}
    </fieldset>
  {/if}

  <label class="setting">
    <span class="caption">Format</span>
    <select class="field" bind:value={draft.format}>
      {#each Object.entries(FORMAT_LABELS) as [value, label] (value)}<option {value}>{label}</option>{/each}
    </select>
  </label>
  {#if draft.format === "number" || draft.format === "percent"}
    <div class="pair">
      <label class="setting"><span class="caption">Decimals</span><input class="field" type="number" min="0" max="10" bind:value={draft.decimals} /></label>
      <label class="setting">
        <span class="caption">Written as</span>
        <select class="field" bind:value={draft.separator}>
          {#each Object.entries(SEPARATOR_LABELS) as [value, label] (value)}<option {value}>{label}</option>{/each}
        </select>
      </label>
    </div>
  {/if}
  {#if draft.format === "number"}
    <div class="pair">
      <label class="setting"><span class="caption">Currency</span><input class="field" placeholder="$" bind:value={draft.currency} /></label>
      <label class="setting">
        <span class="caption">Below zero</span>
        <select class="field" bind:value={draft.negative}>
          {#each Object.entries(NEGATIVE_LABELS) as [value, label] (value)}<option {value}>{label}</option>{/each}
        </select>
      </label>
    </div>
    {#if draft.currency}<label class="check"><input type="checkbox" bind:checked={draft.currencyBefore} /> Currency before the number</label>{/if}
  {:else if draft.format === "date"}
    <label class="setting"><span class="caption">Date written as</span><input class="field" placeholder="dd/mm/yyyy" bind:value={draft.datePattern} /></label>
  {:else if draft.format === "time"}
    <label class="setting">
      <span class="caption">Time written as</span>
      <select class="field" bind:value={draft.timePattern}>{#each TIME_PATTERNS as pattern (pattern)}<option value={pattern}>{pattern}</option>{/each}</select>
    </label>
  {:else if draft.format === "special"}
    <label class="setting">
      <span class="caption">Kind</span>
      <select class="field" bind:value={draft.special}>
        {#each Object.entries(SPECIAL_LABELS) as [value, label] (value)}<option {value}>{label}</option>{/each}
      </select>
    </label>
  {:else if draft.format === "mask"}
    <label class="setting">
      <span class="caption">Pattern</span>
      <input class="field" placeholder="999-AAA" bind:value={draft.mask} />
      <span class="hint">9 is a digit, A a letter, O a letter or digit, X any character; anything else is typed for you.</span>
    </label>
  {/if}

  <fieldset class="setting">
    <legend class="caption">Checks</legend>
    <label class="check"><input type="checkbox" bind:checked={draft.required} /> Required</label>
    <div class="pair">
      <label class="setting"><span class="caption">At least</span><input class="field" inputmode="decimal" bind:value={draft.rangeMin} /></label>
      <label class="setting"><span class="caption">At most</span><input class="field" inputmode="decimal" bind:value={draft.rangeMax} /></label>
    </div>
    <label class="setting"><span class="caption">Most characters</span><input class="field" inputmode="numeric" bind:value={draft.maxLength} /></label>
    <label class="setting">
      <span class="caption">Condition</span>
      <input class="field" placeholder="value <= budget" bind:value={draft.condition} />
      <span class="hint">“value” is this field’s answer. Comparisons, and, or, not.</span>
    </label>
    {#if draft.condition}
      <label class="setting"><span class="caption">Message when it fails</span><input class="field" placeholder="The amount is over the budget." bind:value={draft.conditionMessage} /></label>
    {/if}
  </fieldset>

  <div class="actions">
    <button class="button primary" disabled={problem !== null || !fieldName} onclick={setRules}>Set Rules</button>
  </div>
  <p class="effect" aria-live="polite">
    {problem ??
      "Writes these rules into the field as the standard form functions, then works out every total again. Other PDF apps that run forms use the same rules. Undo takes it back."}
  </p>
{/if}

{#if logic}
  {#each logic.issues as issue, index (index)}<p class="note" role="alert">{issue.message}</p>{/each}
  {#if logic.calculationOrder.length > 0}
    <button class="button" title="Work out every calculated field again from the answers it reads" onclick={workOutAgain}>Work Out Totals Again</button>
  {/if}
  {#if logic.customScriptNote}<p class="note">{logic.customScriptNote}.</p>{/if}
  <details>
    <summary class="hint">Not offered</summary>
    <p class="hint">{logic.noSubmitNote}</p>
    <p class="hint">{logic.conditionalVisibilityNote}</p>
  </details>
{/if}

<style>
  .pair {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 0.5rem;
  }

  .sources {
    display: flex;
    flex-direction: column;
    gap: 0.25rem;
    max-height: 10rem;
    overflow: auto;
  }

  fieldset {
    min-width: 0;
    margin: 0;
    padding: 0;
    border: 0;
    display: flex;
    flex-direction: column;
    gap: 0.375rem;
  }

  .actions {
    display: flex;
    justify-content: flex-end;
  }

  .effect,
  details p {
    margin: 0;
    color: var(--label-secondary);
    font: var(--subheadline);
    line-height: 0.9375rem;
    letter-spacing: var(--subheadline-spacing);
  }

  .note {
    margin: 0;
    padding: 0.375rem 0.5rem;
    border-radius: var(--radius-small);
    background: var(--control);
    font: var(--subheadline);
    line-height: 0.9375rem;
  }

  details p {
    margin-top: 0.25rem;
  }
</style>

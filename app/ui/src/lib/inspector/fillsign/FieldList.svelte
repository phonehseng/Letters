<script lang="ts">
  // every field of the form in the order Tab visits them: its name (or
  // the form's help text), its page, its answer or "Empty", and whether
  // it is required. a click goes to the field on the page, ready to type.
  import type { FillSignSession } from "../../forms/fillSignSession.svelte";
  import { fieldLabel, isFilled } from "../../forms/tabOrder";

  interface Props {
    session: FillSignSession;
  }

  let { session }: Props = $props();

  const rows = $derived(
    session.stops.map((stop) => {
      const field = session.field(stop.field)!;
      return { stop, field, filled: isFilled(field) };
    }),
  );

  function answerOf(field: (typeof rows)[number]["field"], filled: boolean): string {
    if (!filled) return "Empty";
    if (field.kind === "checkbox") return "Ticked";
    if (field.password) return "••••";
    if (field.kind === "dropdown" || field.kind === "list") {
      return field.selected.map((value) => field.options.find((option) => option.value === value)?.label ?? value).join(", ");
    }
    return field.value ?? "";
  }
</script>

<ul class="fields" aria-label="Fields in this form">
  {#each rows as row (row.stop.field)}
    <li>
      <button
        class="field-row"
        class:current={session.active?.field === row.stop.field}
        aria-label={`${fieldLabel(row.field)}, page ${row.stop.page + 1}, ${row.filled ? "filled" : "empty"}${row.field.required ? ", required" : ""}`}
        onclick={() => session.focusField(row.stop.field, row.stop.widget)}
      >
        <span class="name">{fieldLabel(row.field)}{#if row.field.required}<span class="required" aria-hidden="true"> *</span>{/if}</span>
        <span class="page">Page {row.stop.page + 1}</span>
        <span class="answer" class:empty={!row.filled}>{answerOf(row.field, row.filled)}</span>
      </button>
    </li>
  {/each}
</ul>

<style>
  .fields {
    max-height: 16rem;
    margin: 0;
    padding: 0;
    overflow-y: auto;
    list-style: none;
    border-radius: var(--radius-small);
    background: var(--control);
  }

  .field-row {
    display: grid;
    grid-template-columns: 1fr auto;
    gap: 0 0.5rem;
    width: 100%;
    padding: 0.3125rem 0.5rem;
    border: 0;
    background: transparent;
    text-align: left;
  }

  .field-row:hover {
    background: var(--control-hover);
  }

  .field-row.current {
    box-shadow: inset 3px 0 0 var(--accent);
  }

  li + li .field-row {
    border-top: 0.5px solid var(--separator);
  }

  .name {
    overflow: hidden;
    font: var(--body);
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .required {
    color: var(--danger);
  }

  .page,
  .answer {
    color: var(--label-secondary);
    font: var(--subheadline);
    letter-spacing: var(--subheadline-spacing);
  }

  .answer {
    grid-column: 1 / -1;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .answer.empty {
    color: var(--label-tertiary);
  }
</style>

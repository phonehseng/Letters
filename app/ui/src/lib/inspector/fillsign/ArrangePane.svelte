<script lang="ts">
  // Prepare Form > Arrange: putting fields in a group (one name, such as
  // "address", in front of theirs), and putting one field on several
  // pages so a name typed once shows on every page.
  import { groupable, groupEffect, groupNameProblem } from "../../forms/formBuilding";
  import { duplicateAcrossPages, groupFields } from "../../forms/formBuildingApi";
  import { fieldLabel } from "../../forms/tabOrder";
  import type { ToolContext } from "../../inspector";
  import { parsePageRanges } from "../../pageRanges";
  import Choices from "../Choices.svelte";

  interface Props {
    context: ToolContext;
  }

  let { context }: Props = $props();

  const session = $derived(context.fillSign!);
  let task = $state<"group" | "pages">("group");

  // ---- grouping ----
  let group = $state("");
  let members = $state<string[]>([]);
  const candidates = $derived(groupable(session.fields));
  const groupProblem = $derived(groupNameProblem(group, session.fields) ?? (members.length === 0 ? "Choose the fields to put in the group." : null));

  function toggleMember(name: string, on: boolean) {
    const rest = members.filter((each) => each !== name);
    members = on ? [...rest, name] : rest;
  }

  async function makeGroup() {
    const id = context.document.id;
    const name = group.trim();
    const chosen = [...members];
    const worked = await context.change(async () => (await groupFields(id, name, chosen)).document);
    if (worked) {
      members = [];
      context.notify(`${chosen.length} ${chosen.length === 1 ? "field" : "fields"} grouped under “${name}”.`);
    }
  }

  // ---- one field on several pages ----
  let fieldName = $state("");
  let pagesText = $state("");
  const pageCount = $derived(context.document.pages.length);
  const parsed = $derived(parsePageRanges(pagesText, pageCount));
  const copyable = $derived(session.fields.filter((field) => field.kind !== "signature" && field.widgets.length > 0));
  const pagesProblem = $derived(!fieldName ? "Choose a field." : !pagesText.trim() ? "Type the pages, such as 2-5." : parsed.ok ? null : parsed.error);

  async function putOnPages() {
    if (!parsed.ok) return;
    const id = context.document.id;
    const name = fieldName;
    const pages = parsed.pages;
    let added = 0;
    const worked = await context.change(async () => {
      const answer = await duplicateAcrossPages(id, name, pages);
      added = answer.added.length;
      return answer.document;
    });
    // the pages it really went on: one it was already on isn't counted
    if (worked) context.notify(added === 0 ? `“${name}” was already on those pages.` : `“${name}” is now on ${added} more ${added === 1 ? "page" : "pages"}.`);
  }
</script>

<Choices
  label="Arrange"
  hideLabel
  options={[
    { value: "group", label: "Group Fields" },
    { value: "pages", label: "On More Pages" },
  ]}
  bind:value={task}
/>
{#if task === "group"}
  <label class="setting">
    <span class="caption">Group Name</span>
    <input class="field" placeholder="address" bind:value={group} />
  </label>
  <fieldset class="setting list">
    <legend class="caption">Fields</legend>
    {#each candidates as field (field.name)}
      <label class="check">
        <input type="checkbox" checked={members.includes(field.name)} onchange={(event) => toggleMember(field.name, event.currentTarget.checked)} />
        {fieldLabel(field)}
      </label>
    {:else}
      <p class="hint">There are no fields that can be grouped. Fields already in a group can’t join another.</p>
    {/each}
  </fieldset>
  <div class="actions"><button class="button primary" disabled={groupProblem !== null} onclick={makeGroup}>Group Fields</button></div>
  <p class="effect" aria-live="polite">{groupProblem ?? groupEffect(group, members, session.fields)}</p>
{:else}
  <label class="setting">
    <span class="caption">Field</span>
    <select class="field" bind:value={fieldName}>
      <option value="">Choose a field</option>
      {#each copyable as field (field.name)}<option value={field.name}>{fieldLabel(field)}</option>{/each}
    </select>
  </label>
  <label class="setting">
    <span class="caption">Pages</span>
    <input class="field" placeholder="2-{Math.max(2, pageCount)}" bind:value={pagesText} />
  </label>
  <div class="actions"><button class="button primary" disabled={pagesProblem !== null} onclick={putOnPages}>Put on Pages</button></div>
  <p class="effect" aria-live="polite">
    {pagesProblem ?? "Puts the field at the same place on each page. It stays one field: an answer typed on any page shows on all of them."}
  </p>
{/if}

<style>
  fieldset {
    min-width: 0;
    margin: 0;
    padding: 0;
    border: 0;
  }

  .list {
    display: flex;
    flex-direction: column;
    gap: 0.25rem;
    max-height: 10rem;
    overflow: auto;
  }

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

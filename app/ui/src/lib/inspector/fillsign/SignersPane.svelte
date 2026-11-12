<script lang="ts">
  // Prepare Form > Signers: the order the signature fields are signed in,
  // for a form two people sign (a student, then a supervisor, say). the
  // list shows who has signed and who is next; Set Signing Order writes it
  // as the order of the form's fields, which Sign's "Follow the signing
  // order" then keeps to.
  import { untrack } from "svelte";
  import { asCommandError } from "../../api";
  import { moveItem } from "../../forms/formBuilding";
  import { setSigningOrder, signingWorkflow, type WorkflowStep } from "../../forms/signingApi";
  import type { ToolContext } from "../../inspector";

  interface Props {
    context: ToolContext;
  }

  let { context }: Props = $props();

  let steps = $state<WorkflowStep[]>([]);

  $effect(() => {
    const id = context.document.id;
    void context.document.revision;
    untrack(() =>
      signingWorkflow(id)
        .then((read) => (steps = read))
        .catch((problem) => context.showError(asCommandError(problem).message)),
    );
  });

  async function apply() {
    const id = context.document.id;
    const order = steps.map((step) => step.field);
    const worked = await context.change(() => setSigningOrder(id, order));
    if (worked) context.notify("Signing order set.");
  }
</script>

{#if steps.length < 2}
  <p class="hint">Add two or more signature fields (Add Field, Signature) to set the order people sign in.</p>
{:else}
  <ol class="order" aria-label="Signature fields in signing order">
    {#each steps as step, index (step.field)}
      <li>
        <span class="name">{index + 1}. {step.field}</span>
        <span class="state">{step.signed ? `Signed by ${step.signer ?? "someone"}` : step.isNext ? "Next to sign" : "Waiting"}</span>
        <button class="small" aria-label="Move {step.field} up" disabled={index === 0} onclick={() => (steps = moveItem(steps, index, index - 1))}>↑</button>
        <button class="small" aria-label="Move {step.field} down" disabled={index === steps.length - 1} onclick={() => (steps = moveItem(steps, index, index + 1))}>↓</button>
      </li>
    {/each}
  </ol>
  <div class="actions"><button class="button primary" onclick={apply}>Set Signing Order</button></div>
  <p class="effect">Saves this order in the form. When signing with “Follow the signing order”, a field can’t be signed while an earlier one is empty.</p>
{/if}

<style>
  .order {
    display: flex;
    flex-direction: column;
    gap: 0.125rem;
    margin: 0;
    padding: 0;
    list-style: none;
  }

  li {
    display: flex;
    align-items: center;
    gap: 0.375rem;
    padding: 0.25rem;
    border-radius: var(--radius-small);
    background: var(--control);
  }

  .name {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .state {
    color: var(--label-secondary);
    font: var(--subheadline);
  }

  .small {
    border: 0;
    background: transparent;
    color: var(--label-secondary);
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

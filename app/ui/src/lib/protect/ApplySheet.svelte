<script lang="ts">
  // "Apply 3 redactions?": the last question before content is removed.
  // it says how many areas, that it is permanent once saved, and what
  // redaction does not cover, and it holds the two choices of how the
  // boxes look. Cancel is the default: Enter on the sheet does nothing
  // until the person moves to Apply Redactions.
  import { untrack } from "svelte";
  import Choices from "../inspector/Choices.svelte";
  import { applySheetText } from "./protectEffects";
  import ProtectSheet from "./ProtectSheet.svelte";
  import { BLACK, WHITE, type ProtectSession } from "./protectSession.svelte";

  interface Props {
    session: ProtectSession;
    pageCount: number;
  }

  let { session, pageCount }: Props = $props();

  const text = $derived(applySheetText(session.marks, pageCount));
  // the sheet starts from the last choices; changing them here is kept on Apply
  const start = untrack(() => session.options);
  let fill = $state<"black" | "white">(start.fill.r === 255 ? "white" : "black");
  let label = $state(start.overlayText !== null);
  let labelText = $state(start.overlayText ?? "REDACTED");

  function apply() {
    session.options = { ...session.options, fill: fill === "white" ? WHITE : BLACK, overlayText: label ? labelText : null };
    session.apply();
  }
</script>

<ProtectSheet id="apply-redactions" title={text.title} message={text.message} onCancel={() => (session.confirmingApply = false)}>
  <div class="not-covered">
    <p class="caption">Not covered</p>
    <ul>
      {#each text.notCovered as line (line)}
        <li>{line}</li>
      {/each}
    </ul>
  </div>
  <Choices
    label="Box colour"
    options={[
      { value: "black", label: "Black" },
      { value: "white", label: "White" },
    ]}
    bind:value={fill}
  />
  <label class="check">
    <input type="checkbox" bind:checked={label} />
    <span>Write a word in each box</span>
  </label>
  {#if label}
    <input class="field" aria-label="Word in each box" maxlength="40" bind:value={labelText} />
  {/if}
  {#snippet buttons()}
    <button class="button" data-autofocus onclick={() => (session.confirmingApply = false)}>Cancel</button>
    <button class="button primary" onclick={apply}>Apply Redactions</button>
  {/snippet}
</ProtectSheet>

<style>
  .not-covered {
    padding: 0.625rem 0.75rem;
    border-radius: var(--radius-small);
    background: var(--control);
  }

  .not-covered p {
    margin: 0 0 0.25rem;
  }

  ul {
    margin: 0;
    padding-left: 1.1rem;
    color: var(--label-secondary);
    line-height: 1.125rem;
  }
</style>

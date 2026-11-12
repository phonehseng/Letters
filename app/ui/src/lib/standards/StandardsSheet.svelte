<script lang="ts">
  // Prepare & Check…: one sheet from the document menu for the checks a
  // finished PDF goes through before it leaves: PDF/A for archives, Check
  // This PDF, Make Smaller, and Print Check (PDF/X). a sheet rather than a
  // ninth inspector tab: these act on the whole document once, produce a
  // report and maybe a new file, and never edit the page, like Export To….
  // the inspector stays for tools used while working on the pages.
  import { onMount } from "svelte";
  import { asCommandError, type DocumentState } from "../api";
  import Choices from "../inspector/Choices.svelte";
  import OptimizePane from "./OptimizePane.svelte";
  import PdfaPane from "./PdfaPane.svelte";
  import PdfxPane from "./PdfxPane.svelte";
  import PreflightPane from "./PreflightPane.svelte";

  interface Props {
    document: DocumentState;
    /** distance from the top of the window, below the toolbar */
    top: number;
    onClose: () => void;
  }

  let { document: doc, top, onClose }: Props = $props();

  let pane = $state<"pdfa" | "preflight" | "smaller" | "print">("pdfa");
  let busy = $state(false);
  let problem = $state<string | null>(null);
  let sheet = $state<HTMLDivElement>();

  /** runs one check or conversion at a time, and shows its refusal in the sheet. */
  async function run(work: () => Promise<void>) {
    if (busy) return;
    busy = true;
    problem = null;
    try {
      await work();
    } catch (error) {
      problem = asCommandError(error).message;
    } finally {
      busy = false;
    }
  }

  function onKeyDown(event: KeyboardEvent) {
    if (event.key !== "Escape" || busy) return;
    event.preventDefault();
    event.stopPropagation();
    onClose();
  }

  onMount(() => sheet?.focus());
</script>

<!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
<div class="glass sheet" role="dialog" aria-modal="true" aria-label="Prepare and Check" tabindex="-1" style:top="{top + 8}px" bind:this={sheet} onkeydown={onKeyDown}>
  <h2>Prepare & Check “{doc.name}”</h2>
  <Choices
    label="Check"
    hideLabel
    options={[
      { value: "pdfa", label: "PDF/A" },
      { value: "preflight", label: "Check This PDF" },
      { value: "smaller", label: "Make Smaller" },
      { value: "print", label: "Print Check" },
    ]}
    bind:value={pane}
  />
  {#if doc.dirty}<p class="hint">Unsaved changes are included: Letters checks the document as it is shown.</p>{/if}

  {#if pane === "pdfa"}
    <PdfaPane document={doc} {run} {busy} />
  {:else if pane === "preflight"}
    <PreflightPane document={doc} {run} {busy} />
  {:else if pane === "smaller"}
    <OptimizePane document={doc} {run} {busy} />
  {:else}
    <PdfxPane document={doc} {run} {busy} />
  {/if}

  {#if busy}<p class="hint" role="status">Working…</p>{/if}
  {#if problem}<p class="problem" role="alert">{problem}</p>{/if}
  <div class="close"><button class="button" disabled={busy} onclick={onClose}>Done</button></div>
</div>

<style>
  .sheet {
    position: absolute;
    left: 50%;
    z-index: 5;
    display: flex;
    flex-direction: column;
    gap: 0.75rem;
    width: min(36rem, calc(100% - 2rem));
    max-height: calc(100% - 5rem);
    overflow-y: auto;
    padding: 0.875rem 1rem;
    transform: translateX(-50%);
    box-shadow: var(--shadow-sheet);
    outline: none;
  }

  h2 {
    margin: 0;
    font: var(--headline);
  }

  .problem {
    margin: 0;
    color: var(--danger);
  }

  .close {
    display: flex;
    justify-content: flex-end;
  }
</style>

<script lang="ts">
  // "Check Code in This Document": every block of code the document holds,
  // where it is, the language it looks like (changeable), what the checks
  // found, and Open Block in Code Editor. the caveat comes first: code in
  // a document may have lost characters, so the results are a hint.
  import { onMount } from "svelte";
  import { findCodeInDocument, openUntitledCode } from "./codeApi";
  import { DOCUMENT_CODE_CAVEAT, prepareBlock, type PreparedBlock } from "./codeBlocks";
  import { errorText } from "./codeSession.svelte";
  import { checkBlock, extensionFor, foundSentence, type BlockResult } from "./documentCodeCheck";
  import { LANGUAGES, type LanguageId } from "./languages";

  interface Props {
    source: { path: string; kind: "writer" | "pdf"; pdfId: number | null };
    onClose: () => void;
  }

  let { source, onClose }: Props = $props();
  let blocks = $state<PreparedBlock[] | null>(null);
  let results = $state<(BlockResult | null)[]>([]);
  let failure = $state<string | null>(null);
  let closeButton = $state<HTMLButtonElement>();

  async function check(index: number) {
    const block = blocks?.[index];
    if (!block) return;
    results[index] = null;
    results[index] = await checkBlock(block, block.language);
  }

  function changeLanguage(index: number, language: LanguageId) {
    if (!blocks) return;
    blocks[index] = { ...blocks[index], language };
    void check(index);
  }

  onMount(async () => {
    closeButton?.focus();
    try {
      const found = await findCodeInDocument(source.path, source.kind, source.pdfId);
      blocks = found.map(prepareBlock);
      results = blocks.map(() => null);
      blocks.forEach((_, index) => void check(index));
    } catch (problem) {
      failure = errorText(problem);
    }
  });

  function onKeyDown(event: KeyboardEvent) {
    if (event.key === "Escape") {
      event.stopPropagation();
      onClose();
    }
  }
</script>

<div class="backdrop" role="presentation" onkeydown={onKeyDown}>
  <div class="glass sheet" role="dialog" aria-modal="true" aria-labelledby="document-code-title">
    <header>
      <h2 id="document-code-title">Code in This Document</h2>
      <button class="button" bind:this={closeButton} onclick={onClose}>Done</button>
    </header>
    <p class="caveat">{DOCUMENT_CODE_CAVEAT}</p>
    {#if failure}
      <p class="failure" role="alert">{failure}</p>
    {:else if !blocks}
      <p role="status">Looking for code…</p>
    {:else}
      <p role="status">{foundSentence(blocks.length)}</p>
      <ol class="blocks">
        {#each blocks as block, index (index)}
          <li class="block">
            <div class="block-head">
              <strong>{block.location[0].toUpperCase() + block.location.slice(1)}</strong>
              <span class="quiet">found by its {block.foundBy}</span>
              <label>
                <span class="quiet">Looks like</span>
                <select value={block.language} onchange={(event) => changeLanguage(index, (event.target as HTMLSelectElement).value as LanguageId)}>
                  {#each LANGUAGES as language (language.id)}<option value={language.id}>{language.label}</option>{/each}
                </select>
              </label>
              <button class="button" onclick={() => openUntitledCode(block.repaired, extensionFor(block.language)).catch((problem) => (failure = errorText(problem)))}>Open Block in Code Editor</button>
            </div>
            <pre>{block.text}</pre>
            {#if results[index]}
              {@const result = results[index]!}
              <p class="quiet">{result.checkedBy}. {result.problems.length === 0 ? "No problems found." : ""}</p>
              <ul class="problems">
                {#each result.problems as problem, problemIndex (problemIndex)}
                  <li class={problem.severity}>
                    Line {problem.line} of the block: {problem.message}
                    {#if problem.hint}<span class="quiet hint-text">{problem.hint}</span>{/if}
                  </li>
                {/each}
              </ul>
            {:else}
              <p class="quiet" role="status">Checking…</p>
            {/if}
          </li>
        {/each}
      </ol>
    {/if}
  </div>
</div>

<style>
  .backdrop {
    position: absolute;
    inset: 0;
    z-index: 20;
    display: flex;
    justify-content: center;
    align-items: flex-start;
    padding-top: var(--toolbar-space);
    background: rgb(0 0 0 / 0.12);
  }

  .sheet {
    display: flex;
    flex-direction: column;
    width: min(44rem, calc(100% - 2rem));
    max-height: calc(100% - var(--toolbar-space) - 1rem);
    padding: 1rem 1.25rem;
    overflow: auto;
    font: var(--body);
  }

  header,
  .block-head {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 0.5rem 0.75rem;
  }

  header {
    justify-content: space-between;
  }

  h2 {
    margin: 0;
    font: var(--title);
    letter-spacing: var(--title-spacing);
  }

  .caveat,
  .quiet {
    color: var(--label-secondary);
  }

  .failure,
  .error {
    color: var(--danger);
  }

  .blocks {
    margin: 0;
    padding: 0;
    list-style: none;
  }

  .block {
    padding: 0.75rem 0;
    border-top: 0.5px solid var(--separator);
  }

  pre {
    max-height: 12rem;
    margin: 0.5rem 0;
    padding: 0.5rem 0.75rem;
    overflow: auto;
    border-radius: var(--radius-small);
    background: var(--control);
    font: 0.75rem/1.4 ui-monospace, Consolas, Menlo, monospace;
  }

  .problems {
    margin: 0.25rem 0 0;
    padding-left: 1.25rem;
  }

  .hint-text {
    display: block;
  }
</style>

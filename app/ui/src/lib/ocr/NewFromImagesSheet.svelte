<script lang="ts">
  // New from Images…: pictures in, one PDF out. choose or drop PNG, JPEG
  // or TIFF files, put them in order, pick the paper, and optionally have
  // their text read so the PDF can be searched. the PDF opens here,
  // unsaved; nothing is written until Save As.
  //
  // a sheet, like the unsaved changes question: Escape cancels (and stops
  // a run under way), and the window behind waits.
  import { open as showOpenDialog } from "@tauri-apps/plugin-dialog";
  import { onMount, untrack } from "svelte";
  import type { DocumentState } from "../api";
  import Choices from "../inspector/Choices.svelte";
  import { addPictures, makeSentence, moveItem, PAPER_CHOICES, paperSentence } from "./imagesToPdf";
  import { defaultLanguage } from "./languages";
  import { ocrApi, optionsFrom, type OcrStatus, type PaperSize } from "./ocrApi";
  import { rememberNewDocument, suggestedPathFor } from "./newDocuments";

  interface Props {
    /** pictures dropped or opened to start with */
    initial: string[];
    /** the new document is open (unsaved) */
    onOpened: (document: DocumentState) => void;
    onClose: () => void;
  }

  let { initial, onOpened, onClose }: Props = $props();

  // the sheet is made again for other starting pictures (App.svelte keys it), so the first list is enough
  let paths = $state<string[]>(untrack(() => addPictures([], initial)));
  let paper = $state<PaperSize>("fromImage");
  let readText = $state(false);
  let language = $state<string>("eng");
  let status = $state<OcrStatus | null>(null);
  let job = $state<number | null>(null);
  let progress = $state({ done: 0, total: 0 });
  let problem = $state<string | null>(null);
  let sheet = $state<HTMLDivElement>();

  const languageName = $derived(status?.languages.find((each) => each.code === language)?.name ?? language);

  onMount(() => {
    sheet?.focus();
    ocrApi
      .status()
      .then((answer) => {
        status = answer;
        language = defaultLanguage(answer.languages, navigator.language || "en") ?? "eng";
      })
      .catch(() => (status = null));
  });

  const nameOf = (path: string) => path.split(/[\\/]/).pop() ?? path;

  async function choose() {
    const chosen = await showOpenDialog({ multiple: true, filters: [{ name: "Pictures", extensions: ["png", "jpg", "jpeg", "tif", "tiff"] }] });
    const list = Array.isArray(chosen) ? chosen : typeof chosen === "string" ? [chosen] : [];
    paths = addPictures(paths, list);
  }

  async function make() {
    if (paths.length === 0 || job !== null) return;
    problem = null;
    try {
      const options = readText ? optionsFrom([language], { orientation: true, deskew: true, denoise: true }) : null;
      job = await ocrApi.imagesToPdf(paths, paper, options);
      progress = { done: 0, total: paths.length };
      watch(job);
    } catch (error) {
      problem = messageOf(error);
    }
  }

  async function watch(running: number) {
    for (;;) {
      await new Promise((resolve) => setTimeout(resolve, 300));
      if (job !== running) return;
      let answer;
      try {
        answer = await ocrApi.imagesStatus(running);
      } catch (error) {
        problem = messageOf(error);
        job = null;
        return;
      }
      progress = { done: answer.progress.done, total: answer.progress.total };
      if (!answer.finished) continue;
      job = null;
      if (answer.error) {
        problem = answer.error;
        return;
      }
      try {
        const opened = await ocrApi.openNewDocument(running);
        rememberNewDocument(opened.id, suggestedPathFor(paths[0]));
        onOpened(opened);
      } catch (error) {
        problem = messageOf(error);
      }
      return;
    }
  }

  function cancel() {
    if (job !== null) {
      ocrApi.imagesCancel(job).catch(() => {});
      job = null;
    }
    onClose();
  }

  function onKeyDown(event: KeyboardEvent) {
    if (event.key === "Escape") {
      event.preventDefault();
      event.stopPropagation();
      cancel();
    }
  }

  function messageOf(error: unknown): string {
    return typeof error === "object" && error !== null && "message" in error ? String((error as { message: unknown }).message) : String(error);
  }
</script>

<div class="backdrop">
  <div
    class="sheet"
    role="dialog"
    aria-modal="true"
    aria-labelledby="new-from-images-title"
    tabindex="-1"
    bind:this={sheet}
    onkeydown={onKeyDown}
  >
    <h2 id="new-from-images-title">New from Images</h2>
    <p class="hint">Each picture becomes a page, in this order. The pictures themselves don’t change.</p>

    {#if paths.length > 0}
      <ol class="pictures" aria-label="Pictures, in page order">
        {#each paths as path, index (path)}
          <li>
            <span class="name" title={path}>{index + 1}. {nameOf(path)}</span>
            <button class="button" disabled={index === 0} aria-label={`Move ${nameOf(path)} up`} onclick={() => (paths = moveItem(paths, index, index - 1))}>↑</button>
            <button class="button" disabled={index === paths.length - 1} aria-label={`Move ${nameOf(path)} down`} onclick={() => (paths = moveItem(paths, index, index + 1))}>↓</button>
            <button class="button" aria-label={`Remove ${nameOf(path)}`} onclick={() => (paths = paths.filter((each) => each !== path))}>Remove</button>
          </li>
        {/each}
      </ol>
    {/if}
    <button class="button" onclick={choose}>{paths.length > 0 ? "Add Pictures…" : "Choose Pictures…"}</button>

    <Choices label="Paper" options={PAPER_CHOICES} bind:value={paper} />
    <p class="hint">{paperSentence(paper)}</p>

    {#if status?.available}
      <label class="check">
        <input type="checkbox" bind:checked={readText} />
        <span>Make it searchable (recognize the text)</span>
      </label>
      {#if readText}
        <label class="setting">
          <span class="caption">Language of the pictures</span>
          <select class="field" bind:value={language}>
            {#each status.languages as each (each.code)}
              <option value={each.code}>{each.name}</option>
            {/each}
          </select>
        </label>
      {/if}
    {/if}

    {#if job !== null}
      <progress max={progress.total} value={progress.done} aria-label="Pictures done"></progress>
      <p class="hint" aria-live="polite">Making page {Math.min(progress.done + 1, progress.total)} of {progress.total}…</p>
    {:else}
      <p class="hint" aria-live="polite">{makeSentence(paths.length, readText, languageName)}</p>
    {/if}
    {#if problem}<p class="problem" role="alert">{problem}</p>{/if}

    <div class="buttons">
      <span class="space"></span>
      <button class="button" onclick={cancel}>Cancel</button>
      <button class="button primary" disabled={paths.length === 0 || job !== null} onclick={make}>Make PDF</button>
    </div>
  </div>
</div>

<style>
  .backdrop {
    position: fixed;
    inset: 0;
    z-index: 10;
    display: flex;
    justify-content: center;
    align-items: flex-start;
    background: rgb(0 0 0 / 0.12);
  }

  .sheet {
    display: flex;
    flex-direction: column;
    gap: 0.625rem;
    width: min(30rem, calc(100% - 2rem));
    max-height: calc(100% - var(--toolbar-space) - 1rem);
    margin-top: var(--toolbar-space);
    padding: 1.25rem 1.25rem 1rem;
    overflow-y: auto;
    border-radius: var(--radius-large, 14px);
    background: var(--window);
    box-shadow: var(--shadow-glass);
    outline: none;
    animation: drop 0.2s var(--ease);
  }

  h2 {
    margin: 0;
    font: var(--headline);
  }

  .hint,
  .problem {
    margin: 0;
  }

  .pictures {
    display: flex;
    flex-direction: column;
    gap: 0.25rem;
    margin: 0;
    padding: 0;
    list-style: none;
  }

  .pictures li {
    display: flex;
    align-items: center;
    gap: 0.25rem;
  }

  .name {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  progress {
    width: 100%;
  }

  .buttons {
    display: flex;
    gap: 0.5rem;
    margin-top: 0.5rem;
  }

  .space {
    flex: 1;
  }

  @keyframes drop {
    from {
      opacity: 0;
      transform: translateY(-0.75rem);
    }
  }
</style>

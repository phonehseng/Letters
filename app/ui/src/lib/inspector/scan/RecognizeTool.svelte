<script lang="ts">
  // Recognize Text: the language, the pages, the cleanup, then one button.
  // while it runs, the section shows how far it got and a Cancel that
  // really stops; afterwards, what was found and which pages to check.
  import { untrack } from "svelte";
  import type { ToolContext } from "../../inspector";
  import { languageAdvice, MAX_LANGUAGES, toggleLanguage } from "../../ocr/languages";
  import {
    cancelledSentence,
    confidenceSentence,
    lowPagesSentence,
    progressSentence,
    recognizeEffect,
    summarySentence,
  } from "../../ocr/ocrSentences";
  import { availableChoices, defaultChoice, pagesFor, type PageChoice } from "../../ocr/pageRange";
  import Choices from "../Choices.svelte";
  import ToolSection from "../ToolSection.svelte";

  interface Props {
    context: ToolContext;
    open: boolean;
    onToggle: () => void;
  }

  let { context, open, onToggle }: Props = $props();

  const session = $derived(context.scan!);
  const status = $derived(session.status);
  const view = $derived(session.view);
  const pageCount = $derived(context.document.pages.length);
  const facts = $derived({
    current: context.currentPage,
    selection: context.target.fromSelection ? context.target.pages : [],
    pageCount,
    scanned: session.overview?.scanned ?? [],
  });
  const choices = $derived(availableChoices(facts));
  let choice = $state<PageChoice>("current");
  const pages = $derived(pagesFor(choice, facts));
  const running = $derived(session.busy);

  // the banner (or a menu command) may have picked the pages; else a sensible start
  $effect(() => {
    const asked = session.pageChoice;
    // read so the starting choice is made again once the scan overview arrives
    session.overview?.scanned.length;
    untrack(() => {
      choice = asked ?? defaultChoice(facts);
      if (!choices.some((each) => each.value === choice)) choice = choices[0].value;
      session.pageChoice = null;
    });
  });

  const CLEANUP: { key: "deskew" | "denoise" | "orientation"; label: string; explain: string }[] = [
    { key: "deskew", label: "Straighten tilted pages", explain: "Turns a page scanned a little crooked back to level before reading." },
    { key: "denoise", label: "Clean up speckles", explain: "Removes dust and scanner noise that can be read as dots and commas." },
    { key: "orientation", label: "Detect sideways pages", explain: "Finds pages scanned sideways or upside down and reads them the right way up. Adds about a second a page." },
  ];

  function act() {
    if (running) return session.cancel();
    if (view.phase === "done" || view.phase === "cancelled" || view.phase === "failed") session.reset();
    return session.start(pages);
  }
</script>

<ToolSection
  tool="recognizeText"
  {open}
  {onToggle}
  actionLabel={view.phase === "cancelling" ? "Stopping…" : running ? "Cancel" : "Recognize Text"}
  ready={status?.available === true && (running ? view.phase === "running" : pages.length > 0 && session.languages.length > 0)}
  effect={running ? progressSentence(view) : recognizeEffect(pages, pageCount, session.languageWords())}
  onAction={act}
>
  {#if status && !status.available}
    <p class="problem" role="alert">{status.message}</p>
  {:else if status}
    <fieldset class="setting" disabled={running}>
      <legend class="caption">Language of the document</legend>
      <div class="languages">
        {#each status.languages as language (language.code)}
          <label class="check">
            <input
              type="checkbox"
              checked={session.languages.includes(language.code)}
              disabled={!session.languages.includes(language.code) && session.languages.length >= MAX_LANGUAGES}
              onchange={() => (session.languages = toggleLanguage(session.languages, language.code))}
            />
            <span>{language.name}</span>
          </label>
        {/each}
      </div>
      <p class="hint" aria-live="polite">{languageAdvice(session.languages)}</p>
    </fieldset>

    <Choices label="Pages" options={choices} bind:value={choice} />

    <fieldset class="setting" disabled={running}>
      <legend class="caption">Before reading</legend>
      {#each CLEANUP as option (option.key)}
        <label class="check option">
          <input type="checkbox" bind:checked={session.cleanup[option.key]} disabled={option.key === "orientation" && !status.canDetectOrientation} />
          <span>
            {option.label}
            <span class="hint">{option.explain}</span>
          </span>
        </label>
      {/each}
    </fieldset>
  {/if}

  {#if running && (view.phase === "running" || view.phase === "cancelling")}
    <progress max={view.total} value={view.done} aria-label="Pages read"></progress>
  {/if}

  {#if view.phase === "done"}
    <div class="result" role="status">
      <p>{summarySentence(view.summary)}</p>
      {#if confidenceSentence(view.summary)}<p class="hint">{confidenceSentence(view.summary)}</p>{/if}
      {#if lowPagesSentence(view.summary, pageCount)}<p class="hint">{lowPagesSentence(view.summary, pageCount)}</p>{/if}
      <button class="button" onclick={() => (session.showText ? null : session.toggleText())}>Show Recognized Text</button>
    </div>
  {:else if view.phase === "cancelled"}
    <p class="result" role="status">{cancelledSentence(view)}</p>
  {:else if view.phase === "failed"}
    <p class="problem" role="alert">{view.message}</p>
  {/if}
</ToolSection>

<style>
  fieldset {
    min-width: 0;
    margin: 0;
    padding: 0;
    border: 0;
  }

  .languages {
    display: flex;
    flex-wrap: wrap;
    gap: 0.25rem 0.875rem;
  }

  .option {
    align-items: flex-start;
    margin-top: 0.375rem;
  }

  .option > span {
    display: flex;
    flex-direction: column;
  }

  progress {
    width: 100%;
  }

  .result {
    display: flex;
    flex-direction: column;
    gap: 0.25rem;
    margin: 0;
  }

  .result p {
    margin: 0;
  }

  .problem {
    margin: 0;
    color: var(--label);
  }
</style>

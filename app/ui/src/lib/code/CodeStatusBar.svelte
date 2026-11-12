<script lang="ts">
  // the glass strip along the bottom of the code screen: where the cursor
  // is, the problem counts (which open the Problems panel), who checked the
  // file, and the file's language, indentation, encoding and line endings.
  // choices are the system's own pop-up menus, so they type-to-find and
  // read out like menus everywhere.
  import { tooltipFor } from "../tools";
  import { LANGUAGES, type LanguageId } from "./languages";
  import { countProblems, countsSentence, type Problem } from "./problems";
  import { encodingLabel, formatSentence, indentationLabel, lineEndingLabel, type Indentation, type TextFormat } from "./textFormat";

  interface Props {
    line: number;
    column: number;
    problems: Problem[];
    checkerLine: string;
    coverage: string;
    language: LanguageId;
    indentation: Indentation;
    format: TextFormat;
    wrap: boolean;
    onLanguage: (language: LanguageId) => void;
    onIndentation: (indentation: Indentation) => void;
    onWrap: () => void;
    onProblems: () => void;
  }

  let { line, column, problems, checkerLine, coverage, language, indentation, format, wrap, onLanguage, onIndentation, onWrap, onProblems }: Props = $props();

  const counts = $derived(countProblems(problems));
  const INDENT_CHOICES: Indentation[] = [
    { tabs: false, width: 2 },
    { tabs: false, width: 4 },
    { tabs: false, width: 8 },
    { tabs: true, width: 4 },
  ];

  function chooseIndentation(event: Event) {
    const index = Number((event.target as HTMLSelectElement).value);
    onIndentation(INDENT_CHOICES[index]);
  }

  const indentIndex = $derived(INDENT_CHOICES.findIndex((choice) => choice.tabs === indentation.tabs && (choice.tabs || choice.width === indentation.width)));
</script>

<footer class="glass status" aria-label="Status">
  <span class="item" aria-live="off">Ln {line}, Col {column}</span>
  <button class="item link" class:has-errors={counts.errors > 0} onclick={onProblems} title={tooltipFor("codeProblems")}>
    {countsSentence(counts)}
  </button>
  <span class="item quiet" title={coverage}>{checkerLine}</span>
  <span class="spacer"></span>
  <label class="item">
    <span class="visually-hidden">Indentation</span>
    <select value={indentIndex < 0 ? "" : String(indentIndex)} onchange={chooseIndentation} title={tooltipFor("codeIndentation")}>
      {#if indentIndex < 0}<option value="">{indentationLabel(indentation)}</option>{/if}
      {#each INDENT_CHOICES as choice, index (index)}
        <option value={String(index)}>{indentationLabel(choice)}</option>
      {/each}
    </select>
  </label>
  <span class="item quiet" title={formatSentence(format)}>{encodingLabel(format)} · {lineEndingLabel(format.lineEnding)}</span>
  <label class="item">
    <span class="visually-hidden">Language</span>
    <select value={language} onchange={(event) => onLanguage((event.target as HTMLSelectElement).value as LanguageId)} title="The language this file is highlighted and checked as">
      {#each LANGUAGES as choice (choice.id)}
        <option value={choice.id}>{choice.label}</option>
      {/each}
    </select>
  </label>
  <button class="item link" class:off={!wrap} aria-pressed={wrap} onclick={onWrap} title={tooltipFor("codeWrap")}>Wrap</button>
</footer>

<style>
  .status {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 0.25rem 0.875rem;
    margin: 0 0.5rem 0.5rem;
    padding: 0.25rem 0.75rem;
    font: var(--subheadline);
    letter-spacing: var(--subheadline-spacing);
  }

  .item {
    color: var(--label);
    white-space: nowrap;
  }

  .quiet {
    color: var(--label-secondary);
  }

  .spacer {
    flex: 1;
  }

  .link {
    padding: 0.125rem 0.375rem;
    border: 0;
    border-radius: var(--radius-small);
    background: transparent;
    font: inherit;
  }

  .link:hover {
    background: var(--control-hover);
  }

  .has-errors {
    color: var(--danger);
  }

  /* a switch that is off is struck through, as the writer's status bar does */
  .off {
    color: var(--label-secondary);
    text-decoration: line-through;
  }

  select {
    border: 0;
    border-radius: var(--radius-small);
    background: transparent;
    color: inherit;
    font: inherit;
  }

  select:hover {
    background: var(--control-hover);
  }
</style>

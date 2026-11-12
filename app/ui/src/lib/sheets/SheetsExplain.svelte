<script lang="ts">
  // Explain for the spreadsheet screen, on the trailing side like the
  // other screens': first what concerns the active cell (its formula said
  // step by step with each part's value, or why it shows an error with
  // one-click fixes), then every tool on the screen, then the keyboard
  // shortcuts as excel and google sheets have them.
  import { tools, type ToolHelp } from "../tools";
  import type { ErrorFix, ErrorHelp, ExplainStep, Shortcut } from "./sheetsApi";
  import { sheetsTools } from "./sheetsTools";

  interface Props {
    cellName: string;
    steps: ExplainStep[];
    error: ErrorHelp | null;
    shortcuts: Shortcut[];
    onFix: (fix: ErrorFix) => void;
    onClose: () => void;
  }

  let { cellName, steps, error, shortcuts, onFix, onClose }: Props = $props();

  const isMac = navigator.platform.toLowerCase().includes("mac");
  const names = Object.keys(sheetsTools) as (keyof typeof sheetsTools)[];
  const entries = names.map((name) => tools[name] as ToolHelp);
</script>

<aside class="glass explain" aria-label="Explain">
  <header>
    <h2>Explain</h2>
    <button class="button" onclick={onClose}>Close</button>
  </header>

  {#if error}
    <section class="cell">
      <h3>Why {cellName} shows {error.error}</h3>
      <p>{error.meaning}</p>
      <p><strong>Here:</strong> {error.why}</p>
      {#each error.fixes as fix, index (fix.title)}
        <button class="button" class:primary={index === 0} onclick={() => onFix(fix)}>{fix.title}</button>
      {/each}
    </section>
  {/if}

  {#if steps.length > 0}
    <section class="cell">
      <h3>The formula in {cellName}, step by step</h3>
      <ol class="steps">
        {#each steps as step, index (index)}
          <li style:padding-left="{step.depth * 0.75}rem">
            <code>{step.part}</code>
            <span>{step.sentence}</span>
            {#if step.value !== undefined}<span class="value">= {step.value}</span>{/if}
          </li>
        {/each}
      </ol>
    </section>
  {:else if !error}
    <section class="cell">
      <p class="quiet">Select a cell with a formula to see it explained step by step here.</p>
    </section>
  {/if}

  <h3 class="group">On this screen</h3>
  <p class="quiet">{isMac ? "On a Mac, Command takes the place of Ctrl in these keys." : "On a Mac, Command takes the place of Ctrl."}</p>
  {#each entries as entry (entry.label)}
    <section>
      <h4>{entry.label}{entry.shortcut ? ` (${entry.shortcut})` : ""}</h4>
      {#if entry.meaning}<p>{entry.meaning}</p>{/if}
      <p>{entry.whenToUse}</p>
      <p class="quiet"><strong>The file:</strong> {entry.fileEffect}</p>
    </section>
  {/each}

  {#if shortcuts.length > 0}
    <h3 class="group">Keyboard shortcuts</h3>
    <p class="quiet">The same keys as Excel and Google Sheets. Where the two differ, both work here unless one would clash.</p>
    <table>
      <thead><tr><th>What</th><th>Excel</th><th>Google Sheets</th></tr></thead>
      <tbody>
        {#each shortcuts as shortcut (shortcut.operation)}
          <tr>
            <td>{shortcut.description}</td>
            <td><kbd>{isMac ? shortcut.excelMac : shortcut.excelWindows}</kbd></td>
            <td><kbd>{shortcut.googleSheets}</kbd></td>
          </tr>
        {/each}
      </tbody>
    </table>
  {/if}
</aside>

<style>
  .explain {
    width: 22rem;
    max-width: 50vw;
    margin: 0.5rem 0.5rem 0.5rem 0;
    padding: 0.75rem 1rem;
    overflow: auto;
    border-radius: var(--radius-group);
  }

  header {
    display: flex;
    align-items: center;
    justify-content: space-between;
  }

  h2 {
    margin: 0;
    font: var(--title);
    letter-spacing: var(--title-spacing);
  }

  h3 {
    margin: 0.875rem 0 0.25rem;
    font: var(--headline);
  }

  h4 {
    margin: 0.75rem 0 0.125rem;
    font: var(--headline);
    font-weight: 600;
  }

  .group {
    margin-top: 1.25rem;
    padding-top: 0.75rem;
    border-top: 1px solid var(--separator);
  }

  p {
    margin: 0.25rem 0;
    font: var(--body);
  }

  .quiet {
    color: var(--label-secondary);
  }

  .cell .button {
    margin: 0.25rem 0.25rem 0 0;
  }

  .steps {
    margin: 0;
    padding: 0;
    list-style: none;
  }

  .steps li {
    display: flex;
    flex-direction: column;
    gap: 0.125rem;
    margin: 0.375rem 0;
    font: var(--body);
  }

  code {
    color: var(--accent-label);
    font: 400 0.75rem/1.2 ui-monospace, "Cascadia Mono", Consolas, monospace;
    overflow-wrap: anywhere;
  }

  .value {
    color: var(--label-secondary);
    font-variant-numeric: tabular-nums;
  }

  table {
    width: 100%;
    border-collapse: collapse;
    font: var(--subheadline);
  }

  th {
    color: var(--label-secondary);
    font-weight: 600;
    text-align: left;
  }

  td,
  th {
    padding: 0.25rem 0.25rem 0.25rem 0;
    border-bottom: 1px solid var(--separator);
    vertical-align: top;
  }

  kbd {
    font: inherit;
    white-space: nowrap;
  }
</style>

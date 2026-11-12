<script lang="ts">
  // Explain for the code screen: what each tool does, what it does to the
  // file, and what the checks of this file's language look at. a tool that
  // can't be used here (VS Code when it isn't installed) says why.
  import { tools, type ToolHelp } from "../tools";
  import { codeTools } from "./codeTools";

  interface Props {
    coverage: string;
    checkerLine: string;
    vscode: boolean;
    onClose: () => void;
  }

  let { coverage, checkerLine, vscode, onClose }: Props = $props();

  // the screen's own tools, without the two that belong to the viewer and the writer
  const names = (Object.keys(codeTools) as (keyof typeof codeTools)[]).filter((name) => name !== "exportCodeFile" && name !== "checkDocumentCode");
  const entries = $derived(names.map((name) => ({ name, help: tools[name] as ToolHelp, unavailable: name === "codeOpenInVsCode" && !vscode })));
</script>

<aside class="glass explain" aria-label="Explain">
  <header>
    <h2>Explain</h2>
    <button class="button" onclick={onClose}>Close</button>
  </header>
  <section>
    <h3>How this file is checked</h3>
    <p>{checkerLine}. {coverage}</p>
    <p>Nothing you write here is ever run. A compiler, when one is installed, only reads a copy to check the syntax.</p>
  </section>
  {#each entries as entry (entry.name)}
    <section>
      <h3>{entry.help.label}{entry.help.shortcut ? ` (${entry.help.shortcut})` : ""}</h3>
      {#if entry.unavailable}<p class="unavailable">{entry.help.unavailable}</p>{/if}
      {#if entry.help.meaning}<p>{entry.help.meaning}</p>{/if}
      <p>{entry.help.whenToUse}</p>
      <p class="effect"><strong>The file:</strong> {entry.help.fileEffect}</p>
    </section>
  {/each}
</aside>

<style>
  .explain {
    width: 20rem;
    max-width: 50vw;
    margin: 0.5rem 0.5rem 0.5rem 0;
    padding: 0.75rem 1rem;
    overflow: auto;
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

  p {
    margin: 0.25rem 0;
    font: var(--body);
  }

  .effect {
    color: var(--label-secondary);
  }

  .unavailable {
    color: var(--danger);
  }
</style>

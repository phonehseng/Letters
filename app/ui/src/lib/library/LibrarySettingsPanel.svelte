<script lang="ts">
  // what Search All Documents covers, and the index itself: library
  // folders, pinned folders to search, folders never to index, recents,
  // pause, rebuild and delete. it floats beside the search screen, and
  // the Settings window shows the same section (embedded).
  import { onMount } from "svelte";
  import { tools, tooltipFor } from "../tools";
  import { changeLibrary, chooseFolder, controlIndex, libraryOverview, type LibraryChange, type LibraryOverview } from "./libraryApi";
  import { statusSentence } from "./results";

  interface Props {
    /** true inside the Settings window: no close button, no floating */
    embedded?: boolean;
    onClose?: () => void;
    onChanged?: (overview: LibraryOverview) => void;
  }

  let { embedded = false, onClose, onChanged }: Props = $props();

  let overview = $state<LibraryOverview | null>(null);
  let confirmingDelete = $state(false);
  let problem = $state<string | null>(null);

  const settings = $derived(overview?.settings);
  const status = $derived(overview?.status);

  function take(next: LibraryOverview) {
    overview = next;
    onChanged?.(next);
  }

  async function change(action: LibraryChange, path?: string) {
    try {
      take(await changeLibrary(action, path));
    } catch (error) {
      problem = (error as { message?: string })?.message ?? "That didn’t work.";
    }
  }

  async function addFolder(action: "addFolder" | "excludeFolder") {
    const folder = await chooseFolder();
    if (folder) await change(action, folder);
  }

  async function control(action: "pause" | "resume" | "rebuild" | "delete") {
    confirmingDelete = false;
    take(await controlIndex(action));
  }

  function megabytes(bytes: number): string {
    return bytes < 1_000_000 ? `${Math.max(1, Math.round(bytes / 1000))} KB` : `${(bytes / 1_000_000).toFixed(1)} MB`;
  }

  function nameOf(path: string): string {
    return path.split(/[\\/]/).filter(Boolean).at(-1) ?? path;
  }

  onMount(() => {
    libraryOverview().then(take).catch(() => (problem = "Search isn’t available right now."));
    const timer = setInterval(() => libraryOverview().then(take).catch(() => {}), 2000);
    return () => clearInterval(timer);
  });
</script>

<section class="panel" class:glass={!embedded} class:floating={!embedded} aria-labelledby="library-settings-title">
  <header>
    <h2 id="library-settings-title">Search All Documents</h2>
    {#if onClose && !embedded}
      <button class="button" onclick={onClose}>Done</button>
    {/if}
  </header>
  <p class="intro">Letters reads the documents in these places and keeps an index of their words on this computer, in its own data folder. Nothing is sent anywhere, and your files are never changed.</p>

  {#if problem}<p class="problem" role="alert">{problem}</p>{/if}

  {#if settings && status}
    <h3>Folders</h3>
    <ul class="list">
      {#each settings.libraryFolders as folder (folder)}
        <li>
          <span title={folder}>{nameOf(folder)}<small>{folder}</small></span>
          <button class="button" onclick={() => change("removeFolder", folder)} aria-label={`Stop searching ${nameOf(folder)}`}>Remove</button>
        </li>
      {:else}
        <li class="none">No folders yet.</li>
      {/each}
    </ul>
    <button class="button primary" title={tooltipFor("addLibraryFolder")} onclick={() => addFolder("addFolder")}>{tools.addLibraryFolder.label}</button>

    {#if overview && overview.pinnedFolders.length > 0}
      <h3>Pinned Folders</h3>
      <ul class="list">
        {#each overview.pinnedFolders as folder (folder)}
          {@const searched = settings.searchedPinnedFolders.some((f) => f.toLowerCase() === folder.toLowerCase())}
          <li>
            <label>
              <input type="checkbox" checked={searched} onchange={() => change(searched ? "skipPinned" : "searchPinned", folder)} />
              <span title={folder}>Search {nameOf(folder)}</span>
            </label>
          </li>
        {/each}
      </ul>
    {/if}

    <label class="toggle">
      <input type="checkbox" checked={settings.includeRecents} onchange={() => change(settings.includeRecents ? "skipRecents" : "includeRecents")} />
      <span>Search documents in Recents</span>
    </label>

    <h3>Never Indexed</h3>
    <ul class="list">
      {#each settings.excludedFolders as folder (folder)}
        <li>
          <span title={folder}>{nameOf(folder)}<small>{folder}</small></span>
          <button class="button" onclick={() => change("includeFolder", folder)}>Index Again</button>
        </li>
      {:else}
        <li class="none">Every folder above is indexed.</li>
      {/each}
    </ul>
    <button class="button" title={tooltipFor("dontIndexFolder")} onclick={() => addFolder("excludeFolder")}>{tools.dontIndexFolder.label}</button>

    <h3>Index</h3>
    <p class="facts" aria-live="polite">
      {statusSentence(status)}. {status.passages.toLocaleString()} pages and paragraphs, {megabytes(status.bytesOnDisk)} on this computer.
      {#if status.locked > 0}{status.locked} locked {status.locked === 1 ? "file is" : "files are"} searched by name only.{/if}
      {#if status.needsTextRecognition > 0}{status.needsTextRecognition} {status.needsTextRecognition === 1 ? "file has" : "files have"} scanned pages without text.{/if}
      {#if status.recovered}The index was damaged and has been started again.{/if}
    </p>
    <div class="actions">
      {#if settings.paused}
        <button class="button" onclick={() => control("resume")}>Resume Indexing</button>
      {:else}
        <button class="button" title={tooltipFor("pauseIndexing")} onclick={() => control("pause")}>{tools.pauseIndexing.label}</button>
      {/if}
      <button class="button" title={tooltipFor("rebuildIndex")} onclick={() => control("rebuild")}>{tools.rebuildIndex.label}</button>
      {#if confirmingDelete}
        <span class="confirm" role="alert">Delete the index? Your documents are not touched.</span>
        <button class="button" onclick={() => (confirmingDelete = false)}>Cancel</button>
        <button class="button danger" onclick={() => control("delete")}>Delete</button>
      {:else}
        <button class="button" title={tooltipFor("deleteIndex")} onclick={() => (confirmingDelete = true)}>{tools.deleteIndex.label}…</button>
      {/if}
    </div>
  {/if}
</section>

<style>
  .panel {
    display: flex;
    flex-direction: column;
    gap: 0.5rem;
    overflow-y: auto;
  }

  /* beside the results like an inspector, on the trailing side */
  .floating {
    position: absolute;
    top: 3.5rem;
    right: 0.75rem;
    bottom: 0.75rem;
    width: min(24rem, calc(100% - 1.5rem));
    padding: 1rem;
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
  }

  h3 {
    margin: 0.75rem 0 0;
    font: var(--headline);
  }

  .intro,
  .facts,
  .none,
  small {
    color: var(--label-secondary);
    font: var(--subheadline);
  }

  .list {
    margin: 0;
    padding: 0;
    list-style: none;
  }

  .list li {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 0.5rem;
    padding: 0.25rem 0;
  }

  .list span {
    display: flex;
    min-width: 0;
    flex-direction: column;
  }

  small {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .toggle,
  .list label {
    display: flex;
    align-items: center;
    gap: 0.5rem;
  }

  .actions {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 0.5rem;
  }

  .confirm {
    font: var(--subheadline);
  }

  .danger {
    color: var(--danger);
  }

  .problem {
    color: var(--danger);
  }

  .panel > .button {
    align-self: flex-start;
  }
</style>

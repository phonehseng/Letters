<script lang="ts">
  // the home screen: open a file, or pick one you opened before.
  //
  // documents are shown as the pages they are, not as rows in a list,
  // so a student can recognise a file by how it looks.
  import { forgetRecentFile, recentFiles, thumbnailUrl, type RecentFile } from "./api";
  import { matchesSearch, plural, timeAgo } from "./format";
  import HelpPanel from "./HelpPanel.svelte";
  import Icon from "./Icon.svelte";
  import ToolButton from "./ToolButton.svelte";

  interface Props {
    onOpenPath: (path: string) => void;
    onChooseFile: () => void;
    onNewWindow: () => void;
  }

  let { onOpenPath, onChooseFile, onNewWindow }: Props = $props();

  let files = $state<RecentFile[]>([]);
  let search = $state("");
  let showHelp = $state(false);
  const now = Date.now() / 1000;

  const shownFiles = $derived(files.filter((file) => matchesSearch(file.name, search)));

  async function loadFiles() {
    files = await recentFiles();
  }

  async function forget(file: RecentFile) {
    await forgetRecentFile(file.path);
    await loadFiles();
  }

  loadFiles();
</script>

<div class="home">
  <div class="bar" role="toolbar" aria-label="File tools">
    <ToolButton name="open" icon="open" onClick={onChooseFile} />
    <ToolButton name="newWindow" icon="newWindow" onClick={onNewWindow} />

    <label class="search" title="Type part of a file name to find it quickly">
      <Icon name="search" size={0.95} />
      <span class="visually-hidden">Search your recent files</span>
      <input type="search" placeholder="Search" bind:value={search} />
    </label>

    <ToolButton name="help" icon="help" pressed={showHelp} onClick={() => (showHelp = !showHelp)} />
  </div>

  <div class="body">
    <main>
      <h1>Recents</h1>

      <ul class="documents">
        <li>
          <button class="document" onclick={onChooseFile} title="Choose a PDF from your computer">
            <span class="sheet placeholder">
              <Icon name="plus" size={1.75} />
            </span>
            <span class="name">Open a PDF</span>
            <span class="details">or drop one anywhere here</span>
          </button>
        </li>

        {#each shownFiles as file (file.path)}
          <li>
            <button class="document" title={file.path} onclick={() => onOpenPath(file.path)}>
              <span class="sheet">
                <img src={thumbnailUrl(file)} alt="" loading="lazy" />
              </span>
              <span class="name">{file.name}</span>
              <span class="details">
                {plural(file.pageCount, "page")} · {timeAgo(file.openedAt, now)}
              </span>
            </button>
            <button
              class="forget"
              title="Remove from this list. The file itself is not deleted."
              aria-label={`Remove ${file.name} from this list`}
              onclick={() => forget(file)}
            >
              <Icon name="close" size={0.7} />
            </button>
          </li>
        {/each}
      </ul>

      {#if files.length === 0}
        <p class="note">Documents you open will appear here, so you can get back to them in one click.</p>
      {:else if shownFiles.length === 0}
        <p class="note">No recent document has "{search}" in its name.</p>
      {/if}
    </main>

    {#if showHelp}
      <HelpPanel names={["open", "newWindow", "help"]} onClose={() => (showHelp = false)} />
    {/if}
  </div>
</div>

<style>
  .home {
    display: flex;
    flex-direction: column;
    height: 100%;
  }

  .bar {
    display: flex;
    align-items: center;
    gap: 0.25rem;
    padding: 0.35rem 0.75rem;
    border-bottom: 0.5px solid var(--hairline);
    background: var(--bar);
    backdrop-filter: blur(24px) saturate(1.8);
  }

  /* the search field sits on the right, like in a file browser */
  .search {
    display: flex;
    align-items: center;
    gap: 0.35rem;
    width: min(15rem, 40vw);
    margin-left: auto;
    margin-right: 0.5rem;
    padding: 0.3rem 0.6rem;
    border-radius: var(--radius-small);
    background: var(--control);
    color: var(--text-secondary);
  }

  .search:focus-within {
    box-shadow:
      0 0 0 1px var(--accent),
      0 0 0 4px var(--accent-soft);
  }

  .search input {
    flex: 1;
    min-width: 0;
    border: 0;
    outline: 0;
    box-shadow: none;
    background: transparent;
    color: var(--text);
    font-size: 0.8125rem;
  }

  .body {
    display: flex;
    flex: 1;
    min-height: 0;
  }

  main {
    flex: 1;
    overflow-y: auto;
    padding: 2rem 2.5rem 3rem;
  }

  h1 {
    margin: 0 0 1.5rem;
    font-size: 1.75rem;
    font-weight: 700;
    letter-spacing: -0.01em;
  }

  .documents {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(9.5rem, 1fr));
    gap: 2rem 1.75rem;
    margin: 0;
    padding: 0;
    list-style: none;
  }

  .documents li {
    position: relative;
  }

  .document {
    display: flex;
    flex-direction: column;
    align-items: center;
    width: 100%;
    padding: 0;
    border: 0;
    border-radius: var(--radius);
    background: transparent;
    text-align: center;
  }

  /* the picture of the first page, shown as a sheet of paper */
  .sheet {
    display: block;
    width: 100%;
    aspect-ratio: 17 / 22;
    overflow: hidden;
    border-radius: 3px;
    background: #ffffff;
    box-shadow: var(--shadow-card);
    transition:
      transform 0.2s var(--ease),
      box-shadow 0.2s var(--ease);
  }

  .document:hover .sheet {
    transform: translateY(-3px);
    box-shadow: var(--shadow-card-hover);
  }

  .sheet img {
    display: block;
    width: 100%;
    height: 100%;
    object-fit: cover;
    object-position: top;
  }

  .sheet.placeholder {
    display: grid;
    place-items: center;
    border: 1.5px dashed var(--hairline);
    background: var(--control);
    box-shadow: none;
    color: var(--accent);
  }

  .document:hover .sheet.placeholder {
    background: var(--accent-soft);
    box-shadow: none;
  }

  .name {
    max-width: 100%;
    margin-top: 0.7rem;
    overflow: hidden;
    font-size: 0.8125rem;
    font-weight: 500;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .details {
    color: var(--text-secondary);
    font-size: 0.75rem;
  }

  /* only shown when the pointer or keyboard is on the document, to keep the grid calm */
  .forget {
    position: absolute;
    top: -0.45rem;
    left: -0.45rem;
    display: grid;
    place-items: center;
    width: 1.25rem;
    height: 1.25rem;
    border: 0;
    border-radius: 50%;
    background: var(--text-secondary);
    color: var(--window);
    opacity: 0;
    transition: opacity 0.15s var(--ease);
  }

  li:hover .forget,
  .forget:focus-visible {
    opacity: 1;
  }

  .forget:hover {
    background: var(--danger);
  }

  .note {
    margin-top: 2rem;
    color: var(--text-secondary);
    font-size: 0.8125rem;
  }
</style>

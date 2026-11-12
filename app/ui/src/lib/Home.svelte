<script lang="ts">
  // the home screen: open a file, or pick one you opened before.
  //
  // documents are shown as the pages they are, not as rows in a list,
  // so a student can recognise a file by how it looks.
  import { forgetRecentFile, recentFiles, thumbnailUrl, type RecentFile } from "./api";
  import { appCommands } from "./commands";
  import FolderList from "./FolderList.svelte";
  import { matchesSearch, plural, timeAgo } from "./format";
  // word documents show their kind instead of a page count (lib/writer)
  import { recentDetails } from "./writer/routing";
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
  let showExplain = $state(false);
  let searchField = $state<HTMLInputElement>();
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

  // on this screen, find means the search box
  $effect(() =>
    appCommands.register("find", () => {
      searchField?.focus();
      searchField?.select();
    }),
  );
</script>

<div class="home">
  <div class="stage">
    <main>
      <h1>Recents</h1>

      <ul class="documents">
        <li>
          <button class="document" onclick={onChooseFile} title="Choose a PDF from this computer (Ctrl O)">
            <span class="sheet placeholder">
              <Icon name="plus" size={1.75} />
            </span>
            <span class="name">Open a PDF</span>
            <span class="details">or drop one anywhere</span>
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
                {recentDetails(file.path, file.pageCount, (count) => plural(count, "page"))} · {timeAgo(file.openedAt, now)}
              </span>
            </button>
            <button
              class="forget"
              title="Remove from Recents. The file is not deleted."
              aria-label={`Remove ${file.name} from Recents`}
              onclick={() => forget(file)}
            >
              <Icon name="close" size={0.625} />
            </button>
          </li>
        {/each}
      </ul>

      {#if files.length > 0 && shownFiles.length === 0}
        <p class="note">No recent document has “{search}” in its name.</p>
      {/if}

      <FolderList {search} {now} {onOpenPath} />
    </main>

    <div class="toolbar" role="toolbar" aria-label="File tools">
      <div class="glass group">
        <ToolButton name="open" icon="open" onClick={onChooseFile} />
        <ToolButton name="newWindow" icon="newWindow" onClick={onNewWindow} />
      </div>

      <div class="trailing">
        <label class="glass search" title="Type part of a file name to find it">
          <Icon name="search" size={0.9375} />
          <span class="visually-hidden">Search recent documents and pinned folders</span>
          <input type="search" placeholder="Search" bind:value={search} bind:this={searchField} />
        </label>
        <div class="glass group">
          <ToolButton
            name="explain"
            icon="sidePanel"
            pressed={showExplain}
            onClick={() => (showExplain = !showExplain)}
          />
        </div>
      </div>
    </div>
  </div>

  {#if showExplain}
    <HelpPanel groups={[{ title: "On this screen", names: ["open", "newWindow", "pinFolder", "explain"] }]} onClose={() => (showExplain = false)} />
  {/if}
</div>

<style>
  .home {
    display: flex;
    height: 100%;
  }

  .stage {
    position: relative;
    flex: 1;
    min-width: 0;
  }

  main {
    height: 100%;
    overflow-y: auto;
    padding: calc(var(--toolbar-space) + 1.25rem) 2.5rem 3rem;
  }

  h1 {
    margin: 0 0 1.5rem;
    font: var(--large-title);
    letter-spacing: var(--large-title-spacing);
  }

  .toolbar {
    position: absolute;
    inset: 0 0.75rem auto 0;
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: 0.75rem;
    padding: 0.625rem 0 0.625rem 0.75rem;
    pointer-events: none;
  }

  /* content fades out as it slides under the toolbar */
  .toolbar::before {
    content: "";
    position: absolute;
    inset: 0 0 -0.75rem 0;
    background: linear-gradient(to bottom, var(--window) 35%, transparent);
    opacity: 0.9;
  }

  .group,
  .trailing,
  .search {
    position: relative;
    display: flex;
    align-items: center;
    pointer-events: auto;
  }

  .group {
    gap: 0.125rem;
    padding: var(--group-padding);
  }

  .trailing {
    gap: 0.5rem;
  }

  /* a capsule, the shape of a control that stands on its own */
  .search {
    gap: 0.375rem;
    width: min(14rem, 36vw);
    height: 2.25rem;
    padding: 0 0.75rem;
    border-radius: 1.125rem;
    color: var(--label-secondary);
  }

  .search:focus-within {
    box-shadow:
      0 0 0 1px var(--accent),
      0 0 0 4px var(--accent-soft),
      var(--shadow-glass);
  }

  .search input {
    flex: 1;
    min-width: 0;
    border: 0;
    outline: 0;
    box-shadow: none;
    background: transparent;
    color: var(--label);
    font: var(--body);
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
    border-radius: var(--radius-small);
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
    box-shadow: var(--shadow-sheet);
    transition:
      transform 0.2s var(--ease),
      box-shadow 0.2s var(--ease);
  }

  .document:hover .sheet {
    transform: translateY(-2px);
    box-shadow: var(--shadow-sheet-hover);
  }

  .document:active .sheet {
    transform: none;
    box-shadow: var(--shadow-sheet);
  }

  .sheet img {
    display: block;
    width: 100%;
    height: 100%;
    object-fit: cover;
    object-position: top;
  }

  /* the one accent-coloured thing on this screen: the next step */
  .sheet.placeholder {
    display: grid;
    place-items: center;
    border-radius: var(--radius-control);
    background: var(--accent-soft);
    box-shadow: none;
    color: var(--accent-label);
  }

  .document:hover .sheet.placeholder {
    box-shadow: none;
    filter: brightness(0.97);
  }

  .name {
    max-width: 100%;
    margin-top: 0.75rem;
    overflow: hidden;
    font: var(--body);
    font-weight: 500;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .details {
    margin-top: 0.125rem;
    color: var(--label-secondary);
    font: var(--subheadline);
    letter-spacing: var(--subheadline-spacing);
  }

  /* only shown when the pointer or keyboard is on the document, to keep the grid calm */
  .forget {
    position: absolute;
    top: -0.4375rem;
    left: -0.4375rem;
    display: grid;
    place-items: center;
    width: 1.25rem;
    height: 1.25rem;
    border: 0;
    border-radius: 50%;
    background: var(--glass);
    box-shadow: var(--shadow-glass);
    color: var(--label);
    opacity: 0;
    backdrop-filter: var(--blur);
    transition: opacity 0.15s var(--ease);
  }

  li:hover .forget,
  .forget:focus-visible {
    opacity: 1;
  }

  .forget:active {
    background: var(--control-active);
  }

  .note {
    margin-top: 2rem;
    color: var(--label-secondary);
  }
</style>

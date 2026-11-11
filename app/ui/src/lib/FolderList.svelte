<script lang="ts">
  // the "Folders" part of the recents screen: folders the student pinned,
  // such as one per course, each with the pdfs inside it.
  //
  // files are a compact list rather than page pictures: a course folder
  // can hold dozens of readings, and their names are what tells them apart.
  import {
    asCommandError,
    chooseFolder,
    pinFolder,
    pinnedFolders,
    unpinFolder,
    type PinnedFolder,
  } from "./api";
  import { filterFolders } from "./folders";
  import { timeAgo } from "./format";
  import Icon from "./Icon.svelte";
  import { tooltipFor, tools } from "./tools";

  interface Props {
    search: string;
    now: number;
    onOpenPath: (path: string) => void;
  }

  let { search, now, onOpenPath }: Props = $props();

  let folders = $state<PinnedFolder[]>([]);
  let problem = $state<string | null>(null);

  const shown = $derived(filterFolders(folders, search));
  const searching = $derived(search.trim() !== "");

  /** runs a folder command and shows the folders it returns, or what went wrong. */
  async function update(action: () => Promise<PinnedFolder[]>) {
    try {
      folders = await action();
      problem = null;
    } catch (error) {
      problem = asCommandError(error).message;
    }
  }

  async function choosePin() {
    const path = await chooseFolder();
    if (path) await update(() => pinFolder(path));
  }

  update(pinnedFolders);
</script>

<section aria-labelledby="folders-heading">
  <header>
    <h2 id="folders-heading">Folders</h2>
    <button class="button" title={tooltipFor("pinFolder")} onclick={choosePin}>
      {tools.pinFolder.label}
    </button>
  </header>

  {#if problem}
    <p class="note" role="alert">{problem}</p>
  {/if}

  {#if folders.length === 0}
    <p class="note">Pin a folder to see its PDFs here, such as the folder for a course.</p>
  {:else if searching && shown.length === 0}
    <p class="note">No pinned folder has a PDF with “{search}” in its name.</p>
  {/if}

  <div class="folders">
    {#each shown as { folder, files } (folder.path)}
      <div class="folder">
        <div class="folder-name" title={folder.path}>
          <Icon name="open" size={1} />
          <h3>{folder.name}</h3>
          <button
            class="unpin"
            title="Unpin this folder. The folder and its files are not changed."
            aria-label={`Unpin ${folder.name}`}
            onclick={() => update(() => unpinFolder(folder.path))}
          >
            <Icon name="close" size={0.625} />
          </button>
        </div>

        {#if files.length === 0}
          <p class="empty">No PDFs in this folder.</p>
        {:else}
          <ul>
            {#each files as file (file.path)}
              <li>
                <button class="file" title={file.path} onclick={() => onOpenPath(file.path)}>
                  <span class="file-name">{file.name}</span>
                  <span class="when">{timeAgo(file.modifiedAt, now)}</span>
                </button>
              </li>
            {/each}
          </ul>
        {/if}
      </div>
    {/each}
  </div>
</section>

<style>
  section {
    margin-top: 2.75rem;
  }

  header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 1rem;
    margin-bottom: 1rem;
  }

  h2 {
    margin: 0;
    font: var(--title);
    letter-spacing: var(--title-spacing);
  }

  /* folders sit side by side like columns, separated only by space */
  .folders {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(15rem, 1fr));
    gap: 1.5rem 1.75rem;
  }

  .folder-name {
    display: flex;
    align-items: center;
    gap: 0.375rem;
    padding: 0 0.25rem 0.375rem;
    border-bottom: 0.5px solid var(--separator);
    color: var(--label-secondary);
  }

  h3 {
    flex: 1;
    min-width: 0;
    margin: 0;
    overflow: hidden;
    color: var(--label);
    font: var(--headline);
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  /* only shown when the pointer or keyboard is on the folder, to keep the screen calm */
  .unpin {
    display: grid;
    flex-shrink: 0;
    place-items: center;
    width: 1.25rem;
    height: 1.25rem;
    border: 0;
    border-radius: 50%;
    background: var(--control);
    color: var(--label);
    opacity: 0;
    transition: opacity 0.15s var(--ease);
  }

  .folder:hover .unpin,
  .folder:focus-within .unpin {
    opacity: 1;
  }

  .unpin:hover {
    background: var(--control-hover);
  }

  .unpin:active {
    background: var(--control-active);
  }

  ul {
    margin: 0.25rem 0 0;
    padding: 0;
    list-style: none;
  }

  .file {
    display: flex;
    align-items: baseline;
    gap: 0.75rem;
    width: 100%;
    padding: 0.3125rem 0.25rem;
    border: 0;
    border-radius: var(--radius-small);
    background: transparent;
    text-align: left;
    transition: background 0.15s var(--ease);
  }

  .file:hover {
    background: var(--control-hover);
  }

  .file:active {
    background: var(--control-active);
  }

  .file-name {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    font: var(--body);
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .when {
    flex-shrink: 0;
    color: var(--label-secondary);
    font: var(--subheadline);
    letter-spacing: var(--subheadline-spacing);
  }

  .empty,
  .note {
    margin: 0.5rem 0.25rem 0;
    color: var(--label-secondary);
  }

  .note {
    margin: 0 0 1rem;
  }
</style>

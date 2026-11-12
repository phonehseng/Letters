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
  // code files show their language ("C file") instead of a page count (lib/code)
  import { codeFileKind } from "./code/languages";
  import HelpPanel from "./HelpPanel.svelte";
  import Icon from "./Icon.svelte";
  import ToolButton from "./ToolButton.svelte";
  import { tooltipFor } from "./tools";
  import { openLibrarySearch } from "./library/launcher";
  // organising: choose documents with ctrl or shift and move, copy, rename them (lib/organise)
  import { pinnedFolders } from "./api";
  import { clickChoose, keepListed, NOTHING_CHOSEN, type Chosen } from "./organise/organise";
  import OrganiseBar from "./organise/OrganiseBar.svelte";
  // spreadsheets: a blank or starter workbook (lib/sheets)
  import NewSpreadsheetSheet from "./sheets/NewSpreadsheetSheet.svelte";

  interface Props {
    onOpenPath: (path: string) => void;
    onChooseFile: () => void;
    onNewWindow: () => void;
  }

  let { onOpenPath, onChooseFile, onNewWindow }: Props = $props();

  let files = $state<RecentFile[]>([]);
  /** spreadsheets: the New Spreadsheet sheet while it shows */
  let choosingSpreadsheet = $state(false);
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

  // --- organising (lib/organise)
  let chosen = $state<Chosen>(NOTHING_CHOSEN);
  let pinned = $state<string[]>([]);
  /** goes up after a move so the pinned folders read their files again */
  let foldersVersion = $state(0);
  let organiseRequest = $state<{ action: "rename" | "move" | "recycle"; at: number } | null>(null);

  async function loadPinned() {
    pinned = (await pinnedFolders()).map((folder) => folder.path);
  }
  loadPinned();

  /** a click on a tile: with ctrl, command or shift it chooses, plain it opens. */
  function onTileClick(event: MouseEvent, path: string) {
    const toggle = event.ctrlKey || event.metaKey;
    const next = clickChoose(chosen, shownFiles.map((f) => f.path), path, { toggle, range: event.shiftKey });
    if (next) chosen = next;
    else onOpenPath(path);
  }

  async function afterOrganise() {
    await loadFiles();
    foldersVersion += 1;
    chosen = keepListed(chosen, files.map((f) => f.path));
  }

  /** F2 renames, ctrl shift m moves, delete recycles (with a question first). */
  function organiseKeys(event: KeyboardEvent) {
    if (chosen.paths.length === 0 || (event.target as HTMLElement)?.tagName === "INPUT") return;
    const action = event.key === "F2" ? "rename" : event.key === "Delete" ? "recycle" : (event.ctrlKey || event.metaKey) && event.shiftKey && event.key.toLowerCase() === "m" ? "move" : null;
    if (!action) return;
    event.preventDefault();
    organiseRequest = { action, at: performance.now() };
  }

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
        <!-- scan: photos or scans to a new pdf (lib/ocr/NewFromImagesSheet.svelte) -->
        <li>
          <button class="document" onclick={() => appCommands.dispatch("new-from-images", "pointer")} title="Make a PDF from photos or scans">
            <span class="sheet placeholder quiet">
              <Icon name="plus" size={1.75} />
            </span>
            <span class="name">New from Images…</span>
            <span class="details">photos or scans to a PDF</span>
          </button>
        </li>        <!-- spreadsheets: a blank or starter workbook in the sheets screen (lib/sheets) -->
        <li>
          <button class="document" onclick={() => (choosingSpreadsheet = true)} title={tooltipFor("sheetsNewSpreadsheet")}>
            <span class="sheet placeholder quiet">
              <Icon name="plus" size={1.75} />
            </span>
            <span class="name">New Spreadsheet…</span>
            <span class="details">blank, budget, grades and more</span>
          </button>
        </li>

        {#each shownFiles as file (file.path)}
          <li>
            <button class="document" class:chosen={chosen.paths.includes(file.path)} aria-pressed={chosen.paths.includes(file.path)} title={file.path} onclick={(event) => onTileClick(event, file.path)}>
              <span class="sheet">
                <img src={thumbnailUrl(file)} alt="" loading="lazy" />
              </span>
              <span class="name">{file.name}</span>
              <span class="details">
                {codeFileKind(file.path) ?? recentDetails(file.path, file.pageCount, (count) => plural(count, "page"))} · {timeAgo(file.openedAt, now)}
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

      <!-- organising: read again after a move so the folders show where files went -->
      {#key foldersVersion}
        <FolderList {search} {now} {onOpenPath} />
      {/key}
    </main>

    <OrganiseBar chosen={chosen.paths} {pinned} request={organiseRequest} onChanged={afterOrganise} onClear={() => (chosen = NOTHING_CHOSEN)} />

    <!-- accessibility: the toolbar sits in the window's banner, so every part is in a landmark -->
    <header>
    <div class="toolbar" role="toolbar" aria-label="File tools">
      <div class="glass group">
        <ToolButton name="open" icon="open" onClick={onChooseFile} />
        <ToolButton name="newWindow" icon="newWindow" onClick={onNewWindow} />
        <!-- compare: choose (or drop) two versions; the window shows the Compare screen -->
        <ToolButton name="compareTwoPdfs" icon="twoPages" onClick={() => appCommands.dispatch("compare-two-pdfs", "pointer")} />
      </div>

      <div class="trailing">
        <label class="glass search" title="Type part of a file name to find it">
          <Icon name="search" size={0.9375} />
          <span class="visually-hidden">Search recent documents and pinned folders</span>
          <input type="search" placeholder="Search" bind:value={search} bind:this={searchField} />
        </label>
        <!-- search all documents: the scope switch; Everywhere searches inside documents (lib/library) -->
        <div class="glass group" role="group" aria-label="Search in">
          <ToolButton name="searchEverywhere" icon="search" onClick={() => openLibrarySearch(search)} />
        </div>
        <div class="glass group">
          <!-- settings: the gear of Recents (lib/settings) -->
          <ToolButton name="settings" icon="tools" onClick={() => appCommands.dispatch("open-settings", "pointer")} />
          <ToolButton
            name="explain"
            icon="sidePanel"
            pressed={showExplain}
            onClick={() => (showExplain = !showExplain)}
          />
        </div>
      </div>
    </div>
    </header>
  </div>

  {#if showExplain}
    <HelpPanel groups={[{ title: "On this screen", names: ["open", "newWindow", "compareTwoPdfs", "newFromImages", "sheetsNewSpreadsheet", "pinFolder", "searchEverywhere", "moveToFolder", "copyToFolder", "renameDocument", "removeFromRecents", "moveToRecycleBin", "settings", "explain"] }]} onClose={() => (showExplain = false)} />
  {/if}
</div>

<svelte:window onkeydown={organiseKeys} />

<!-- spreadsheets: the starter workbooks (lib/sheets) -->
{#if choosingSpreadsheet}
  <NewSpreadsheetSheet onClose={() => (choosingSpreadsheet = false)} />
{/if}

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

  /* organising: a chosen document gets the accent ring, as a selection does */
  .document.chosen .sheet {
    box-shadow:
      0 0 0 3px var(--accent),
      var(--shadow-sheet);
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

  /* scan: New from Images is a second way in, so it stays quiet: no accent */
  .sheet.placeholder.quiet {
    background: var(--control);
    color: var(--label-secondary);
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
    top: -0.5625rem;
    left: -0.5625rem;
    display: grid;
    place-items: center;
    /* accessibility: 24 px, the smallest target wcag 2.2 asks for */
    width: 1.5rem;
    height: 1.5rem;
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

<script lang="ts">
  // the bar that appears when documents are chosen in Recents: Move to
  // Folder…, Copy to…, Rename, Reveal, Remove from Recents and Move to
  // Recycle Bin, with the name clash question and an Undo that really
  // moves them back. the rules and words are in organise.ts.
  import { onMount } from "svelte";
  import { forgetRecentFile } from "../api";
  import { libraryOverview } from "../library/libraryApi";
  import { tooltipFor } from "../tools";
  import FolderPicker from "./FolderPicker.svelte";
  import { conflictQuestion, doneSentence, nameOf, recycleQuestion, rememberDestination } from "./organise";
  import { copyDocuments, moveDocuments, recycleDocuments, renameDocument, revealDocument, undoOrganise, type Conflict, type Done, type OrganiseReport } from "./organiseApi";

  interface Props {
    chosen: string[];
    pinned: string[];
    /** the lists changed on disk: read them again */
    onChanged: () => void;
    onClear: () => void;
    /** set from outside (F2, Ctrl Shift M, Delete) to start an action */
    request?: { action: "rename" | "move" | "recycle"; at: number } | null;
  }

  let { chosen, pinned, onChanged, onClear, request = null }: Props = $props();

  const RECENT_KEY = "letters.organise.recentFolders";
  const isMac = navigator.platform.toLowerCase().includes("mac");

  let picking = $state<"move" | "copy" | null>(null);
  let renaming = $state<string | null>(null);
  let newName = $state("");
  let confirmRecycle = $state(false);
  let clash = $state<{ report: OrganiseReport; folder: string; kind: "move" | "copy" } | null>(null);
  let toast = $state<{ text: string; done: Done[] } | null>(null);
  let problem = $state<string | null>(null);
  let library = $state<string[]>([]);
  let recent = $state<string[]>(readRecent());
  let announcement = $state("");

  function readRecent(): string[] {
    try {
      return JSON.parse(localStorage.getItem(RECENT_KEY) ?? "[]");
    } catch {
      return [];
    }
  }

  function remember(folder: string) {
    recent = rememberDestination(recent, folder);
    try {
      localStorage.setItem(RECENT_KEY, JSON.stringify(recent));
    } catch {}
  }

  function finish(report: OrganiseReport, kind: "moved" | "copied" | "renamed", folder: string, earlier: Done[] = []) {
    const done = [...earlier, ...report.done];
    problem = report.problem;
    if (done.length) {
      toast = { text: doneSentence(kind, done.length, folder), done };
      announcement = toast.text;
    }
    onChanged();
  }

  async function run(kind: "move" | "copy", paths: string[], folder: string, conflict: Conflict, earlier: Done[] = []) {
    const report = kind === "move" ? await moveDocuments(paths, folder, conflict) : await copyDocuments(paths, folder, conflict);
    if (report.conflict) {
      clash = { report: { ...report, done: [...earlier, ...report.done] }, folder, kind };
      return;
    }
    finish(report, kind === "move" ? "moved" : "copied", folder, earlier);
    onClear();
  }

  async function pick(folder: string) {
    const kind = picking!;
    picking = null;
    remember(folder);
    await run(kind, chosen, folder, "ask");
  }

  /** Keep Both, Replace or Skip for the clashing file, then the rest as before. */
  async function resolve(choice: Conflict) {
    const current = clash!;
    clash = null;
    const { path, remaining } = current.report.conflict!;
    const first = current.kind === "move" ? await moveDocuments([path], current.folder, choice) : await copyDocuments([path], current.folder, choice);
    const earlier = [...current.report.done, ...first.done];
    if (remaining.length) await run(current.kind, remaining, current.folder, "ask", earlier);
    else finish({ ...first, done: [] }, current.kind === "move" ? "moved" : "copied", current.folder, earlier);
  }

  async function rename() {
    const path = renaming!;
    renaming = null;
    const report = await renameDocument(path, newName);
    if (report.conflict) problem = conflictQuestion(report.conflict.existing).split(".")[0] + ". Choose another name.";
    else finish(report, "renamed", "");
  }

  async function recycle() {
    confirmRecycle = false;
    try {
      const count = await recycleDocuments(chosen);
      announcement = count === 1 ? "Moved to the Recycle Bin." : `${count} documents moved to the Recycle Bin.`;
      onClear();
      onChanged();
    } catch (error) {
      problem = (error as { message?: string })?.message ?? "That didn’t work.";
    }
  }

  async function removeFromRecents() {
    for (const path of chosen) await forgetRecentFile(path);
    announcement = "Removed from Recents. The files are not deleted.";
    onClear();
    onChanged();
  }

  async function undo() {
    const done = toast!.done;
    toast = null;
    const report = await undoOrganise(done);
    problem = report.problem;
    announcement = report.problem ?? "Undone.";
    onChanged();
  }

  function startRename() {
    renaming = chosen[0];
    newName = nameOf(chosen[0]).replace(/\.[^.]+$/, "");
  }

  // keys from Home: F2, Ctrl Shift M, Delete
  $effect(() => {
    if (!request || chosen.length === 0) return;
    if (request.action === "rename") startRename();
    else if (request.action === "move") picking = "move";
    else confirmRecycle = true;
  });

  onMount(() => {
    libraryOverview()
      .then((o) => (library = o.settings.libraryFolders))
      .catch(() => {});
  });
</script>

{#if chosen.length > 0}
  <div class="glass bar" role="toolbar" aria-label="Organise documents">
    <span class="count">{chosen.length === 1 ? nameOf(chosen[0]) : `${chosen.length} documents`}</span>
    <button class="button" title={tooltipFor("moveToFolder")} onclick={() => (picking = "move")}>Move to Folder…</button>
    <button class="button" title={tooltipFor("copyToFolder")} onclick={() => (picking = "copy")}>Copy to…</button>
    {#if chosen.length === 1}
      <button class="button" title={tooltipFor("renameDocument")} onclick={startRename}>Rename</button>
      <button class="button" title={tooltipFor("revealDocument")} onclick={() => revealDocument(chosen[0])}>{isMac ? "Show in Finder" : "Show in File Explorer"}</button>
    {/if}
    <button class="button" title={tooltipFor("removeFromRecents")} onclick={removeFromRecents}>Remove from Recents</button>
    <button class="button" title={tooltipFor("moveToRecycleBin")} onclick={() => (confirmRecycle = true)}>{isMac ? "Move to Trash" : "Move to Recycle Bin"}</button>
    <button class="button" onclick={onClear}>Done</button>
  </div>
{/if}

{#if picking}
  <FolderPicker title={picking === "move" ? "Move to Folder" : "Copy to Folder"} {pinned} {library} {recent} onPick={pick} onClose={() => (picking = null)} />
{/if}

{#if renaming}
  <form class="glass sheet" onsubmit={(event) => (event.preventDefault(), rename())}>
    <label>
      <span>Rename “{nameOf(renaming)}”</span>
      <!-- svelte-ignore a11y_autofocus -->
      <input class="field" bind:value={newName} autofocus onkeydown={(event) => event.key === "Escape" && (renaming = null)} />
    </label>
    <button class="button" type="button" onclick={() => (renaming = null)}>Cancel</button>
    <button class="button primary" type="submit">Rename</button>
  </form>
{/if}

{#if clash}
  <div class="glass sheet" role="alertdialog" aria-label="Name in use">
    <p>{conflictQuestion(clash.report.conflict!.existing)}</p>
    <button class="button" onclick={() => resolve("skip")}>Skip</button>
    <button class="button" onclick={() => resolve("replace")}>Replace</button>
    <button class="button primary" onclick={() => resolve("keepBoth")}>Keep Both</button>
  </div>
{/if}

{#if confirmRecycle}
  <div class="glass sheet" role="alertdialog" aria-label="Move to Recycle Bin">
    <p>{recycleQuestion(chosen, isMac)}</p>
    <button class="button" onclick={() => (confirmRecycle = false)}>Cancel</button>
    <button class="button primary" onclick={recycle}>{isMac ? "Move to Trash" : "Move to Recycle Bin"}</button>
  </div>
{/if}

{#if toast}
  <div class="glass toast" role="status">
    <span>{toast.text}</span>
    <button class="button" onclick={undo}>Undo</button>
    <button class="button" onclick={() => (toast = null)}>OK</button>
  </div>
{/if}

{#if problem}
  <div class="glass toast" role="alert">
    <span>{problem}</span>
    <button class="button" onclick={() => (problem = null)}>OK</button>
  </div>
{/if}

<div class="visually-hidden" aria-live="polite">{announcement}</div>

<style>
  .bar,
  .toast,
  .sheet {
    position: absolute;
    left: 50%;
    z-index: 4;
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 0.5rem;
    max-width: calc(100% - 2rem);
    padding: 0.5rem 0.75rem;
    border-radius: var(--radius-group);
    transform: translateX(-50%);
  }

  .bar {
    bottom: 1rem;
  }

  .toast {
    bottom: 4.5rem;
  }

  .sheet {
    top: 4rem;
    max-width: min(30rem, calc(100% - 2rem));
  }

  .sheet p {
    flex-basis: 100%;
    margin: 0 0 0.25rem;
  }

  .sheet label {
    display: flex;
    flex-direction: column;
    flex-basis: 100%;
    gap: 0.375rem;
  }

  .count {
    font: var(--headline);
  }
</style>

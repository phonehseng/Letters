<script lang="ts">
  // one window: either the home screen or one open document.
  import { getCurrentWebview } from "@tauri-apps/api/webview";
  import { getCurrentWindow } from "@tauri-apps/api/window";
  import {
    asCommandError,
    chooseFile,
    closeDocument,
    newWindow,
    openDocument,
    takeStartupFile,
    type OpenedDocument,
  } from "./lib/api";
  import Home from "./lib/Home.svelte";
  import Viewer from "./lib/Viewer.svelte";

  let openedDocument = $state<OpenedDocument | null>(null);
  let errorMessage = $state<string | null>(null);
  /** the locked file we are asking a password for, if any */
  let lockedPath = $state<string | null>(null);
  let password = $state("");
  let wrongPassword = $state(false);

  /**
   * opens a file in this window, or in a new window if this one already
   * shows a document: every window keeps its own document.
   */
  async function openPath(path: string, withPassword?: string) {
    if (openedDocument) {
      await newWindow(path);
      return;
    }
    errorMessage = null;
    try {
      openedDocument = await openDocument(path, withPassword);
      lockedPath = null;
      password = "";
      await getCurrentWindow().setTitle(`${openedDocument.name} - Letters`);
    } catch (problem) {
      const error = asCommandError(problem);
      if (error.kind === "password-required") {
        wrongPassword = withPassword !== undefined;
        lockedPath = path;
      } else {
        errorMessage = error.message;
      }
    }
  }

  async function chooseAndOpen() {
    const path = await chooseFile();
    if (path) await openPath(path);
  }

  async function goHome() {
    if (!openedDocument) return;
    await closeDocument(openedDocument.id);
    openedDocument = null;
    await getCurrentWindow().setTitle("Letters");
  }

  function submitPassword(event: SubmitEvent) {
    event.preventDefault();
    if (lockedPath) openPath(lockedPath, password);
  }

  // a file this window was started for (for example by double clicking it)
  takeStartupFile().then((path) => {
    if (path) openPath(path);
  });

  // files dropped anywhere on the window: the first opens here (or in a
  // new window if a document is already open), the rest in new windows
  $effect(() => {
    const stopListening = getCurrentWebview().onDragDropEvent((event) => {
      if (event.payload.type !== "drop") return;
      const [first, ...others] = event.payload.paths;
      if (first) openPath(first);
      others.forEach((path) => newWindow(path));
    });
    return () => {
      stopListening.then((stop) => stop());
    };
  });
</script>

{#if errorMessage}
  <div class="banner" role="alert">
    <span>{errorMessage}</span>
    <button class="button" onclick={() => (errorMessage = null)}>Dismiss</button>
  </div>
{/if}

{#if lockedPath}
  <form class="banner" onsubmit={submitPassword}>
    <label>
      {wrongPassword ? "That password did not work. Try again:" : "This PDF is locked. Enter its password:"}
      <!-- svelte-ignore a11y_autofocus -->
      <input class="field" type="password" bind:value={password} autofocus />
    </label>
    <button class="button primary" type="submit">Unlock</button>
    <button class="button" type="button" onclick={() => (lockedPath = null)}>Cancel</button>
  </form>
{/if}

<div class="screen">
  {#if openedDocument}
    {#key openedDocument.id}
      <Viewer document={openedDocument} onHome={goHome} onNewWindow={() => newWindow()} />
    {/key}
  {:else}
    <Home onOpenPath={openPath} onChooseFile={chooseAndOpen} onNewWindow={() => newWindow()} />
  {/if}
</div>

<style>
  :global(#app) {
    display: flex;
    flex-direction: column;
  }

  .screen {
    flex: 1;
    min-height: 0;
  }

  .banner {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    justify-content: center;
    gap: 0.75rem;
    padding: 0.5rem 1rem;
    border-bottom: 0.5px solid var(--hairline);
    background: var(--surface);
    font-size: 0.8125rem;
  }

  .banner[role="alert"] {
    color: var(--danger);
  }

  .banner label {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 0.5rem;
  }
</style>

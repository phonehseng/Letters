<script lang="ts">
  // one window: either the recents screen or one open document.
  import { getCurrentWebview } from "@tauri-apps/api/webview";
  import { getCurrentWindow } from "@tauri-apps/api/window";
  import {
    asCommandError,
    chooseFile,
    closeDocument,
    listenForMenuCommands,
    newWindow,
    openDocument,
    takeStartupFile,
    type OpenedDocument,
  } from "./lib/api";
  import { appCommands, commandForKey, isCommand, keyPressOf } from "./lib/commands";
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

  async function showRecents() {
    if (!openedDocument) return;
    await closeDocument(openedDocument.id);
    openedDocument = null;
  }

  function submitPassword(event: SubmitEvent) {
    event.preventDefault();
    if (lockedPath) openPath(lockedPath, password);
  }

  /**
   * every keyboard shortcut of the window arrives here and goes to the
   * shared dispatcher, the same way the menu bar's commands do.
   */
  function onKeyDown(event: KeyboardEvent) {
    const command = commandForKey(keyPressOf(event));
    if (!command) return;
    // even a shortcut this screen does not use must not reach the web
    // view, or ctrl + plus would zoom the whole interface
    event.preventDefault();
    appCommands.dispatch(command, "keyboard");
  }

  // commands that work on every screen. the viewer and the recents
  // screen add their own while they are shown.
  $effect(() => {
    const takeBack = [
      appCommands.register("open", chooseAndOpen),
      appCommands.register("new-window", () => newWindow()),
    ];
    return () => takeBack.forEach((unregister) => unregister());
  });

  // the menu bar (macOS only) sends its commands as events
  $effect(() =>
    listenForMenuCommands((command) => {
      if (isCommand(command)) appCommands.dispatch(command, "menu");
    }),
  );

  // a window is named after what it shows, never after the app, so
  // several windows can be told apart in the taskbar
  $effect(() => {
    getCurrentWindow().setTitle(openedDocument ? openedDocument.name : "Recents");
  });

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

<svelte:window onkeydown={onKeyDown} />

<div class="screen">
  {#if openedDocument}
    {#key openedDocument.id}
      <Viewer document={openedDocument} onRecents={showRecents} onNewWindow={() => newWindow()} />
    {/key}
  {:else}
    <Home onOpenPath={openPath} onChooseFile={chooseAndOpen} onNewWindow={() => newWindow()} />
  {/if}
</div>

<!-- messages float just under the toolbar and never push the document around -->
{#if errorMessage}
  <div class="glass message" role="alert">
    <span>{errorMessage}</span>
    <button class="button" onclick={() => (errorMessage = null)}>OK</button>
  </div>
{/if}

{#if lockedPath}
  <form class="glass message" onsubmit={submitPassword}>
    <label>
      <span>{wrongPassword ? "That password didn’t work. Try again." : "This PDF is locked. Enter its password."}</span>
      <!-- svelte-ignore a11y_autofocus -->
      <input class="field" type="password" placeholder="Password" bind:value={password} autofocus />
    </label>
    <button class="button" type="button" onclick={() => (lockedPath = null)}>Cancel</button>
    <button class="button primary" type="submit">Unlock</button>
  </form>
{/if}

<style>
  :global(#app) {
    position: relative;
  }

  .screen {
    height: 100%;
  }

  .message {
    position: absolute;
    top: calc(var(--toolbar-space) + 0.75rem);
    left: 50%;
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 0.75rem;
    max-width: min(34rem, calc(100% - 2rem));
    padding: 0.625rem 0.625rem 0.625rem 1rem;
    transform: translateX(-50%);
    animation: arrive 0.2s var(--ease);
  }

  .message label {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 0.625rem;
  }

  /* a short slide down from the toolbar shows where the message came from */
  @keyframes arrive {
    from {
      opacity: 0;
      transform: translate(-50%, -0.5rem);
    }
  }
</style>

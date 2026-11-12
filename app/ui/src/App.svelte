<script lang="ts">
  // one window: either the recents screen or one open document.
  import { getCurrentWebview } from "@tauri-apps/api/webview";
  import { getCurrentWindow } from "@tauri-apps/api/window";
  import {
    asCommandError,
    chooseFile,
    chooseSaveLocation,
    closeDocument,
    listenForMenuCommands,
    newWindow,
    openDocument,
    saveDocument,
    saveDocumentAs,
    takeStartupFile,
    type DocumentState,
  } from "./lib/api";
  import { appCommands, commandForKey, isCommand, isTextField, keyPressOf, type Command } from "./lib/commands";
  import { isAvailable } from "./lib/documentMenu";
  import { documentChanges, mayLeave, windowTitle, type SaveChoice } from "./lib/documentState";
  import Home from "./lib/Home.svelte";
  import SaveChangesSheet from "./lib/SaveChangesSheet.svelte";
  import Viewer from "./lib/Viewer.svelte";

  let openedDocument = $state<DocumentState | null>(null);
  let errorMessage = $state<string | null>(null);
  /** the locked file we are asking a password for, if any */
  let lockedPath = $state<string | null>(null);
  let password = $state("");
  let wrongPassword = $state(false);
  /** the unsaved changes question while it is showing, with who is waiting for the answer */
  let saveQuestion = $state<{ name: string; answer: (choice: SaveChoice) => void } | null>(null);

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
    if (!openedDocument || !(await mayLeaveDocument())) return;
    await closeDocument(openedDocument.id);
    openedDocument = null;
  }

  /** takes a new state from the shell, if it is still about the document shown. */
  function acceptState(next: DocumentState) {
    if (openedDocument && openedDocument.id === next.id) openedDocument = next;
  }

  /**
   * saves the document to its own file. returns false when it could not
   * be saved; the message then says what to do (often: Save As).
   */
  function save(): Promise<boolean> {
    const shown = openedDocument;
    if (!shown) return Promise.resolve(false);
    return documentChanges(async () => {
      try {
        acceptState(await saveDocument(shown.id));
        return true;
      } catch (problem) {
        errorMessage = asCommandError(problem).message;
        return false;
      }
    });
  }

  /** asks where to save a copy, then saves it there. the document then belongs to the copy. */
  async function saveAs(): Promise<boolean> {
    const shown = openedDocument;
    if (!shown) return false;
    const path = await chooseSaveLocation(shown.path);
    if (!path) return false;
    return documentChanges(async () => {
      try {
        acceptState(await saveDocumentAs(shown.id, path));
        return true;
      } catch (problem) {
        errorMessage = asCommandError(problem).message;
        return false;
      }
    });
  }

  /** shows the unsaved changes sheet and waits for the answer. */
  function askToSave(name: string): Promise<SaveChoice> {
    return new Promise((resolve) => {
      saveQuestion = {
        name,
        answer: (choice) => {
          saveQuestion = null;
          resolve(choice);
        },
      };
    });
  }

  /** whether the document may be closed, after asking about unsaved changes. */
  async function mayLeaveDocument(): Promise<boolean> {
    // wait for edits still on their way, so the question sees them
    await documentChanges(async () => {});
    return mayLeave(openedDocument, askToSave, save);
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
    // view, or ctrl + plus would zoom the whole interface and ctrl + r
    // would reload it
    event.preventDefault();
    // while the save question is open, the window behind it waits
    if (saveQuestion) return;
    appCommands.dispatch(command, "keyboard");
  }

  /**
   * the menu bar's undo and redo reach us even while someone types in a
   * text field. there they belong to the field, as the standard menu
   * items would have done (see app/shell/src/menu.rs).
   */
  function onMenuCommand(command: Command) {
    if (saveQuestion) return;
    const forTheField = (command === "undo" || command === "redo") && isTextField(document.activeElement);
    if (forTheField) {
      document.execCommand(command);
      return;
    }
    appCommands.dispatch(command, "menu");
  }

  // commands that work on every screen. the viewer and the recents
  // screen add their own while they are shown.
  $effect(() => {
    const takeBack = [
      appCommands.register("open", chooseAndOpen),
      appCommands.register("new-window", () => newWindow()),
      appCommands.register("save", () => {
        if (openedDocument && isAvailable("save", openedDocument)) save();
      }),
      appCommands.register("save-as", () => {
        if (openedDocument) saveAs();
      }),
    ];
    return () => takeBack.forEach((unregister) => unregister());
  });

  // the menu bar (macOS only) sends its commands as events
  $effect(() =>
    listenForMenuCommands((command) => {
      if (isCommand(command)) onMenuCommand(command);
    }),
  );

  // closing the window with unsaved changes asks first. if the answer is
  // cancel, or saving fails, the window stays open
  $effect(() => {
    const stopListening = getCurrentWindow().onCloseRequested(async (event) => {
      if (!(await mayLeaveDocument())) event.preventDefault();
    });
    return () => {
      stopListening.then((stop) => stop());
    };
  });

  // a window is named after what it shows, never after the app, so
  // several windows can be told apart in the taskbar. an edited document
  // says so, as on a mac
  $effect(() => {
    getCurrentWindow().setTitle(windowTitle(openedDocument));
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
      <Viewer
        document={openedDocument}
        onDocumentChange={acceptState}
        onError={(message) => (errorMessage = message)}
        onRecents={showRecents}
        onNewWindow={() => newWindow()}
      />
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

{#if saveQuestion}
  <SaveChangesSheet name={saveQuestion.name} onChoose={saveQuestion.answer} />
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

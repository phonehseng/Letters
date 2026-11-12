<script lang="ts">
  // one window: either the recents screen or one open document.
  import { getCurrentWebview } from "@tauri-apps/api/webview";
  import { getCurrentWindow } from "@tauri-apps/api/window";
  import {
    asCommandError,
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
  import { fileDropTarget, toCssPoint } from "./lib/fileDrop";
  import Home from "./lib/Home.svelte";
  import SaveChangesSheet from "./lib/SaveChangesSheet.svelte";
  import Viewer from "./lib/Viewer.svelte";
  // word-processor mode: .docx and .odt open in the writer (lib/writer)
  import { routeForPath, unsupportedMessage } from "./lib/writer/routing";
  import { openWriterDocument, type WriterOpened } from "./lib/writer/writerApi";
  import { writerGuard } from "./lib/writer/writerGuard";
  import { printPages } from "./lib/writer/print";
  import WriterScreen from "./lib/writer/WriterScreen.svelte";
  // protect: the question before marks for redaction are lost (lib/protect)
  import { protectMayLeave } from "./lib/protect/protectGuard";
  // compare: its own screen over the document or Recents (lib/compare)
  import CompareScreen from "./lib/compare/CompareScreen.svelte";
  import { compareCommands } from "./lib/compare/compareKeys";
  import { EMPTY_SETUP, setupFromDocument, type CompareSetup } from "./lib/compare/setup";
  // scan: New from Images, and pictures opened or dropped (lib/ocr)
  import { isPicturePath, picturePathsAmong } from "./lib/ocr/imagesToPdf";
  import NewFromImagesSheet from "./lib/ocr/NewFromImagesSheet.svelte";
  import { forgetNewDocument, newDocumentPath } from "./lib/ocr/newDocuments";
  // conversion and export, settings: the Settings window, "Open PDF as
  // Document…", spreadsheets and presentations shown as pdfs (lib/export, lib/settings)
  import { openPdfAsDocumentWindow, openPdfFileAsDocument, showOfficeAsPdf, takeOpenAsDocument } from "./lib/export/exportApi";
  import { isShownAsPdf, shownAsPdfSentence } from "./lib/export/openByKind";
  import { openSettingsWindow, SETTINGS_WINDOW } from "./lib/settings/settingsApi";
  import SettingsScreen from "./lib/settings/SettingsScreen.svelte";
  import { chooseFile as choosePdf } from "./lib/api";
  // search all documents: its own screen over the window (lib/library)
  import { setArrival } from "./lib/library/arrival";
  import { takePendingQuery } from "./lib/library/launcher";
  import type { DocumentHit, PassageHit } from "./lib/library/libraryApi";
  import LibrarySearchScreen from "./lib/library/LibrarySearchScreen.svelte";
  import { listen } from "@tauri-apps/api/event";
  import { documentState } from "./lib/api";
  // code and text files: the code editor screen, loaded only when one opens (lib/code)
  import { chooseAnyFileToOpen, openCodeDocument, type CodeDocument } from "./lib/code/codeApi";
  import { isCodePath } from "./lib/code/languages";
  import DocumentCodeTools from "./lib/code/DocumentCodeTools.svelte";

  let openedDocument = $state<DocumentState | null>(null);
  /** the Compare screen while it shows; the document (or Recents) waits underneath */
  let comparing = $state<CompareSetup | null>(null);
  /** commands go to the Compare screen while it shows, else to the window's screen */
  const dispatcher = () => (comparing ? compareCommands : appCommands);
  let openedWriter = $state<WriterOpened | null>(null);
  let errorMessage = $state<string | null>(null);
  /** the locked file we are asking a password for, if any */
  let lockedPath = $state<string | null>(null);
  let password = $state("");
  let wrongPassword = $state(false);
  /** the unsaved changes question while it is showing, with who is waiting for the answer */
  let saveQuestion = $state<{ name: string; answer: (choice: SaveChoice) => void } | null>(null);
  /** scan: the New from Images sheet while it shows, with the pictures it starts with */
  let newFromImages = $state<string[] | null>(null);
  /** code: the code or text file this window shows, if any */
  let openedCode = $state<CodeDocument | null>(null);

  /** scan: the pdf New from Images made is open (unsaved); it takes this window's place */
  async function showNewDocument(next: DocumentState) {
    newFromImages = null;
    if (openedDocument && !(await mayLeaveDocument())) {
      await closeDocument(next.id);
      return;
    }
    if (openedDocument) await closeDocument(openedDocument.id);
    openedWriter = null;
    openedDocument = next;
  }

  /**
   * opens a file in this window, or in a new window if this one already
   * shows a document: every window keeps its own document.
   */
  async function openPath(path: string, withPassword?: string) {
    // scan: a picture is offered as a new pdf (New from Images)
    if (isPicturePath(path)) {
      newFromImages = [path];
      return;
    }
    if (openedDocument || openedWriter || openedCode) {
      await newWindow(path);
      return;
    }
    errorMessage = null;
    // export: a window opened by "Open PDF as Document…" shows the pdf as a document
    if (await takeOpenAsDocument(path)) {
      await openAsDocument(path);
      return;
    }
    const route = routeForPath(path);
    if (route !== "viewer") {
      // export: a spreadsheet or presentation is shown as a pdf office lays out
      if (route === "unsupported" && isShownAsPdf(path)) await openAsPdf(path);
      // code: code and text files, and any other file that turns out to be text
      else if (route === "unsupported") await openInCode(path);
      else await openInWriter(path);
      return;
    }
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
    // code: the Open panel also offers code and text files
    const path = await chooseAnyFileToOpen();
    if (path) await openPath(path);
  }

  /** code: a code or text file in the code editor; anything that isn't text gets the usual sentence. */
  async function openInCode(path: string) {
    try {
      openedCode = await openCodeDocument(path);
    } catch (problem) {
      errorMessage = isCodePath(path) ? asCommandError(problem).message : unsupportedMessage(path);
    }
  }

  async function closeCode() {
    if (openedCode && (await mayLeaveDocument())) openedCode = null;
  }

  async function showRecents() {
    if (!openedDocument || !(await mayLeaveDocument())) return;
    await closeDocument(openedDocument.id);
    openedDocument = null;
  }

  // --- conversion and export (lib/export)

  /** a pdf rebuilt as an editable document, unsaved, in the writer. */
  async function openAsDocument(path: string) {
    try {
      openedWriter = await openPdfFileAsDocument(path);
    } catch (problem) {
      errorMessage = asCommandError(problem).message;
    }
  }

  /** a spreadsheet or presentation as a pdf Office or LibreOffice made. */
  async function openAsPdf(path: string) {
    try {
      const made = await showOfficeAsPdf(path);
      openedDocument = await openDocument(made.pdf);
      errorMessage = shownAsPdfSentence(path, made.program);
    } catch (problem) {
      errorMessage = asCommandError(problem).message;
    }
  }

  /** "Open PDF as Document…": the pdf on screen, or one chosen, in a new window. */
  async function chooseAndOpenAsDocument() {
    const path = openedDocument?.path ?? (await choosePdf());
    if (path) await openPdfAsDocumentWindow(path);
  }

  /** a .docx or .odt: the writer shows it (it closes its own session when it goes). */
  async function openInWriter(path: string) {
    try {
      openedWriter = await openWriterDocument(path);
    } catch (problem) {
      errorMessage = asCommandError(problem).message;
    }
  }

  async function closeWriter() {
    if (openedWriter && (await mayLeaveDocument())) openedWriter = null;
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
    // scan: a pdf New from Images made has no file yet, so Save asks where
    if (newDocumentPath(shown.id)) return saveAs();
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
    const path = await chooseSaveLocation(newDocumentPath(shown.id) ?? shown.path);
    if (!path) return false;
    return documentChanges(async () => {
      try {
        acceptState(await saveDocumentAs(shown.id, path));
        forgetNewDocument(shown.id);
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
    // code: the code screen answers through the same guard as the writer
    if ((openedWriter || openedCode) && writerGuard.mayLeave) return writerGuard.mayLeave(askToSave);
    // protect: marks for redaction not yet applied are offered first
    if (openedDocument && !(await protectMayLeave())) return false;
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
    dispatcher().dispatch(command, "keyboard");
  }

  /**
   * the menu bar's undo and redo reach us even while someone types in a
   * text field. there they belong to the field, as the standard menu
   * items would have done (see app/shell/src/menu.rs).
   */
  function onMenuCommand(command: Command) {
    if (saveQuestion) return;
    const typing = isTextField(document.activeElement);
    // the writer's page is a text field with its own undo history (lib/writer)
    // and so is the code editor (lib/code), which keeps its own history too
    const inWriterPage = !!document.activeElement?.closest(".ProseMirror, .cm-editor");
    if (typing && !inWriterPage && (command === "undo" || command === "redo")) {
      document.execCommand(command);
      return;
    }
    // option + up and down move the cursor in a text field; pages stay put
    if (typing && (command === "move-pages-up" || command === "move-pages-down")) return;
    dispatcher().dispatch(command, "menu");
  }

  /** shows the Compare screen; Back (onClose) returns to what was there. */
  function openCompare(setup: CompareSetup) {
    errorMessage = null;
    comparing = setup;
  }

  /** the saved report opens in the viewer: here from Recents, or in a new window. */
  function openFromCompare(path: string) {
    comparing = null;
    openPath(path);
  }

  // commands that work on every screen. the viewer and the recents
  // screen add their own while they are shown.
  $effect(() => {
    const takeBack = [appCommands.register("open", chooseAndOpen), appCommands.register("new-window", () => newWindow())];
    // the writer registers its own save, save as and print while it shows (and so does the code screen)
    if (!openedWriter && !openedCode) {
      takeBack.push(
        appCommands.register("save", () => {
          if (openedDocument && isAvailable("save", openedDocument)) save();
        }),
        appCommands.register("save-as", () => {
          if (openedDocument) saveAs();
        }),
        // printing a pdf: its pages at printer resolution (lib/writer/print.ts)
        appCommands.register("print", () => {
          if (openedDocument) printPages(openedDocument.id, openedDocument.pages, openedDocument.revision);
        }),
      );
    }
    return () => takeBack.forEach((unregister) => unregister());
  });

  // compare: with a document open it is the newer version; from Recents both are chosen
  $effect(() => {
    if (openedWriter || openedCode) return;
    const takeBack = [
      appCommands.register("compare-with-version", () => {
        if (openedDocument) openCompare(setupFromDocument(openedDocument));
      }),
      appCommands.register("compare-two-pdfs", () => openCompare(EMPTY_SETUP)),
    ];
    return () => takeBack.forEach((unregister) => unregister());
  });

  // scan: New from Images… works from Recents and from a document
  $effect(() => appCommands.register("new-from-images", () => (newFromImages = [])));

  // --- search all documents (lib/library)

  /** the search screen while it shows, with the words it starts with */
  let searchingAll = $state<string | null>(null);

  $effect(() => appCommands.register("search-all-documents", () => (searchingAll = takePendingQuery())));

  // organising: an open pdf whose file was moved shows its new name and saves there (lib/organise)
  $effect(() => {
    const stopListening = listen<{ ids: number[] }>("document-moved", async (event) => {
      if (openedDocument && event.payload.ids.includes(openedDocument.id)) acceptState(await documentState(openedDocument.id));
    });
    return () => {
      stopListening.then((stop) => stop());
    };
  });

  /** a result opens at its page, with its words in the document's Find bar. */
  function openHit(hit: DocumentHit, passage: PassageHit | null, query: string, inNewWindow: boolean) {
    const page = hit.kind === "pdf" ? (passage?.page ?? 0) : 0;
    setArrival(hit.path, { page, block: hit.kind === "word" ? (passage?.number ?? null) : null, query });
    if (inNewWindow) {
      newWindow(hit.path);
      return;
    }
    searchingAll = null;
    openPath(hit.path);
  }

  // export and settings: Settings… and Open PDF as Document… work on every screen
  $effect(() => {
    const takeBack = [appCommands.register("open-settings", () => openSettingsWindow()), appCommands.register("open-pdf-as-document", chooseAndOpenAsDocument)];
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
    // the writer names the window itself while it shows, and so do compare and the code screen
    if (openedWriter || comparing || openedCode) return;
    // the Settings window is called Settings (lib/settings)
    if (getCurrentWindow().label === SETTINGS_WINDOW) return;
    getCurrentWindow().setTitle(windowTitle(openedDocument));
  });

  // a file this window was started for (for example by double clicking it)
  takeStartupFile().then((path) => {
    if (path) openPath(path);
  });

  // files dropped on the window: a part of the window (the thumbnails)
  // may claim them first, to insert their pages where they land.
  // otherwise the first opens here (or in a new window if a document is
  // already open), the rest in new windows
  $effect(() => {
    const stopListening = getCurrentWebview().onDragDropEvent((event) => {
      const payload = event.payload;
      const target = fileDropTarget();
      if (payload.type === "leave") {
        target?.leave();
        return;
      }
      const point = toCssPoint(payload.position, window.devicePixelRatio);
      if (payload.type === "over" || payload.type === "enter") {
        if (point) target?.over(point.x, point.y);
        return;
      }
      if (target?.drop(payload.paths, point)) return;
      // scan: pictures dropped together become one new pdf
      const pictures = picturePathsAmong(payload.paths);
      if (pictures.length > 0 && pictures.length === payload.paths.length) {
        newFromImages = [...(newFromImages ?? []), ...pictures];
        return;
      }
      const [first, ...others] = payload.paths;
      if (first) openPath(first);
      others.forEach((path) => newWindow(path));
    });
    return () => {
      stopListening.then((stop) => stop());
    };
  });
</script>

<svelte:window onkeydown={onKeyDown} />

<!-- compare: the screen underneath stays as it was (scroll, tools, marks) but out of sight and out of reach -->
<!-- accessibility: while the save question is open the screen behind is inert too, so Tab can't leave the sheet -->
<div class="screen" class:waiting={!!comparing || searchingAll !== null} inert={!!comparing || !!saveQuestion || searchingAll !== null}>
  {#if getCurrentWindow().label === SETTINGS_WINDOW}
    <!-- the Settings window (lib/settings) -->
    <SettingsScreen />
  {:else if openedDocument}
    {#key openedDocument.id}
      <Viewer
        document={openedDocument}
        onDocumentChange={acceptState}
        onError={(message) => (errorMessage = message)}
        onRecents={showRecents}
        onNewWindow={() => newWindow()}
      />
    {/key}
  {:else if openedWriter}
    {#key openedWriter.id}
      <WriterScreen
        opened={openedWriter}
        onError={(message) => (errorMessage = message)}
        onRecents={closeWriter}
        onNewWindow={() => newWindow()}
      />
    {/key}
  {:else if openedCode}
    <!-- code: the code editor screen and codemirror load only now, in their own chunk (lib/code) -->
    {#await import("./lib/code/CodeScreen.svelte") then screen}
      <screen.default document={openedCode} onError={(message: string) => (errorMessage = message)} onRecents={closeCode} onNewWindow={() => newWindow()} />
    {/await}
  {:else}
    <Home onOpenPath={openPath} onChooseFile={chooseAndOpen} onNewWindow={() => newWindow()} />
  {/if}
  <!-- code: Export as Code File… and Check Code in This Document, beside a pdf or a word document -->
  {#if openedDocument}
    <DocumentCodeTools source={{ path: openedDocument.path, kind: "pdf", pdfId: openedDocument.id, edited: openedDocument.dirty }} />
  {:else if openedWriter}
    <DocumentCodeTools source={{ path: openedWriter.path, kind: "writer", pdfId: null, edited: false }} />
  {/if}
</div>

<!-- search all documents: over the window like compare; Back returns to what was there -->
{#if searchingAll !== null}
  <div class="screen compare-screen">
    <LibrarySearchScreen initialQuery={searchingAll} onClose={() => (searchingAll = null)} onOpen={openHit} />
  </div>
{/if}

{#if comparing}
  <div class="screen compare-screen">
    <CompareScreen setup={comparing} onClose={() => (comparing = null)} onOpenPdf={openFromCompare} />
  </div>
{/if}

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

{#if newFromImages}
  {#key newFromImages.join("|")}
    <NewFromImagesSheet initial={newFromImages} onOpened={showNewDocument} onClose={() => (newFromImages = null)} />
  {/key}
{/if}

<style>
  :global(#app) {
    position: relative;
  }

  .screen {
    height: 100%;
  }

  /* compare: the screen underneath keeps its size and scroll, but is not shown */
  .screen.waiting {
    visibility: hidden;
  }

  .compare-screen {
    position: absolute;
    inset: 0;
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

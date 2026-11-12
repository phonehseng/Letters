<script lang="ts">
  // the writer: one .docx or .odt document as pages on the desk, with
  // the floating toolbar, the outline on the leading side, the Format
  // inspector and Explain on the trailing side, and the status bar.
  // the panels share the window by the same rules as the viewer's
  // (sidePanels.ts). commands from the menu bar and the keyboard arrive
  // through the app's one dispatcher (commands.ts).
  import { getCurrentWindow } from "@tauri-apps/api/window";
  import { onDestroy, onMount, tick, untrack } from "svelte";
  import { appCommands, keyPressOf, type Command } from "../commands";
  import { remInPixels } from "../dom";
  import { zoomIn, zoomOut } from "../layout";
  import { closePanel, fitPanels, inspectorFloats, togglePanel, openPanel, type SidePanel } from "../sidePanels";
  import { modelToDoc } from "./convert";
  import * as edit from "./editing";
  import { writerExplainGroups, type WriterTab } from "./explainList";
  import FindReplace from "./FindReplace.svelte";
  import type { DocumentMeta } from "./model";
  import PageSurface from "./PageSurface.svelte";
  import WriterNotices from "./WriterNotices.svelte";
  import Ruler from "./Ruler.svelte";
  import { writerCommandForKey } from "./shortcuts";
  import StatusBar from "./StatusBar.svelte";
  import { choiceForParagraph, styleHeadingLevel } from "./styles";
  import { unitForLocale, twipsToPx } from "./units";
  import * as api from "./writerApi";
  import { WRITER_COMMANDS, type WriterCommand } from "./writerCommands";
  import WriterExplain from "./WriterExplain.svelte";
  import { writerGuard, writerMayLeave } from "./writerGuard";
  import WriterInspector from "./WriterInspector.svelte";
  import WriterOutline from "./WriterOutline.svelte";
  import { WriterSession, errorText } from "./writerSession.svelte";
  import WriterToolbar from "./WriterToolbar.svelte";
  import DriveSheet from "./DriveSheet.svelte";
  import ChangedOnDiskSheet, { type ChangedChoice } from "../ChangedOnDiskSheet.svelte";
  import ObjectFrame from "./ObjectFrame.svelte";
  import CompatibilitySheet from "./CompatibilitySheet.svelte";
  import { exportPdf as exportDocumentPdf, printDocument, saveAs as saveDocumentAs } from "./fileActions";
  import { chooseProvider, providerExplanation, providerLabel, type EditingMode } from "./layoutSource";
  import { LayoutCheck } from "./layoutCheck.svelte";
  import { arriveAt, followMoves } from "./neighbours";
  import { WritingGoals } from "./writingGoals.svelte";
  import { documentFonts, fontsInText, missingFonts, replaceFont } from "./fonts";
  import { openFontLibrary, setDocumentFonts, useFontInDocument } from "../fonts";
  import { compatibilityLine, switchQuestion } from "./strictMode";

  interface Props {
    opened: api.WriterOpened;
    onError: (message: string) => void;
    onRecents: () => void;
    onNewWindow: () => void;
  }

  let { opened, onError, onRecents, onNewWindow }: Props = $props();

  let session = $state(untrack(() => new WriterSession(opened, onError)));
  let recovery = $state(untrack(() => opened.recovery));
  let zoom = $state(1);
  let followsWidth = true;
  let nightMode = $state(false);
  let rulerShown = $state(true);
  let panels = $state<SidePanel[]>([]);
  let inspectorTab = $state<WriterTab>("format");
  let findOpen = $state(false);
  let replaceShown = $state(false);
  let findBar = $state<FindReplace>();
  let prompt = $state<"link" | "comment" | null>(null);
  let odtQuestion = $state<{ text: string; path: string } | null>(null);
  let focusComment = $state<string | null>(null);
  let pageCount = $state(1);
  let currentPage = $state(1);
  let toolbarHeight = $state(0);
  let viewportWidth = $state(0);
  let windowWidth = $state(window.innerWidth);
  let remPx = $state(remInPixels());
  let rulerSentences = $state<string[]>([]);
  let objectFrame = $state<ObjectFrame>();
  /** words a search result asked Find to start with */
  let findQuery = $state("");
  /** Focus Mode: the page alone, the controls faded until pointed at */
  let focusMode = $state(false);
  // the goals line in the status bar (writingGoals.svelte.ts)
  const goals = new WritingGoals();

  const unit = unitForLocale(navigator.language || "en-US");
  const editor = $derived(session.editor);
  const meta = $derived(editor.doc.attrs.meta as DocumentMeta);
  const page = $derived(meta.sections?.[0]?.page ?? { width: 12240, height: 15840, margins: { top: 1440, right: 1440, bottom: 1440, left: 1440 } });
  const dirty = $derived(session.dirty);
  const showSidebar = $derived(panels.includes("sidebar"));
  const showInspector = $derived(panels.includes("inspector"));
  const showExplain = $derived(panels.includes("explain"));
  const floatingInspector = $derived(inspectorFloats(windowWidth, remPx));
  const narrow = $derived(viewportWidth > 0 && viewportWidth < 44 * remPx);
  const current = $derived(edit.currentParagraphProps(editor));
  const styleKey = $derived(
    current
      ? choiceForParagraph(meta.styles, current.data.styleId, current.node.type.name === "heading" ? (current.node.attrs.level as number) : current.data.styleId ? styleHeadingLevel(meta.styles, current.data.styleId) : null)
      : "body",
  );
  const counts = $derived(session.counts());
  const explainGroups = $derived(writerExplainGroups(showInspector ? inspectorTab : null));

  // --- panels and zoom

  function toggle(panel: SidePanel) {
    panels = togglePanel(panels, panel, windowWidth, remPx);
  }

  function openInspector(tab: WriterTab) {
    inspectorTab = tab;
    panels = openPanel(panels, "inspector", windowWidth, remPx);
  }

  function fitWidth() {
    followsWidth = true;
    const available = viewportWidth - 48;
    zoom = Math.min(1.5, Math.max(0.5, available / twipsToPx(page.width)));
  }

  function setZoom(next: number) {
    followsWidth = false;
    zoom = Math.min(4, Math.max(0.25, next));
  }

  // --- saving

  // Save As, Export as PDF and Print (fileActions.ts)
  const saveAs = () => saveDocumentAs(session, onError, (question) => (odtQuestion = question));
  const exportPdf = () => exportDocumentPdf(session);
  const print = () => printDocument(session);

  // --- editing mode and where page breaks come from (layoutSource.ts)

  let wordInstalled = $state(false);
  // Word's and the engine's lines and pages (layoutCheck.svelte.ts)
  const layoutCheck = new LayoutCheck(() => session);
  let clock = $state(Date.now());
  let compatibilityOpen = $state(false);
  const mode = $derived<EditingMode>(session.settings.mode === "strict" ? "strict" : "free");
  const provider = $derived(chooseProvider(mode, { word: wordInstalled, engine: !!session.settings.layoutEngine }));
  const usedFonts = $derived(documentFonts(meta, fontsInText(editor.doc)));
  const missing = $derived(missingFonts(usedFonts));

  // fonts the library has but the system doesn't (added, bundled, downloaded)
  // are loaded into the editor from their bytes, so the text shows in them
  let loadedFonts = new Set<string>();
  $effect(() => {
    for (const name of usedFonts) {
      if (loadedFonts.has(name)) continue;
      loadedFonts.add(name);
      useFontInDocument(name).catch(() => false);
    }
  });

  /** Replace in the Missing Fonts view: one font for another everywhere, one undo step. */
  function onReplaceFont(from: string, to: string) {
    session.run((state) => replaceFont(state, from, to), false);
    session.announce(`“${from}” replaced with “${to}”.`);
  }

  // what Ctrl T's Font Library knows about this document
  $effect(() => setDocumentFonts({ documentFonts: usedFonts, onReplaceFont }));
  const issues = $derived(mode === "strict" ? session.issues : []);

  /** Strict Word mode asks first to fix whatever isn't Word-exact. */
  async function setMode(next: EditingMode) {
    session.setSettings({ ...session.settings, mode: next });
    if (next === "free") {
      session.announce("Free mode: every Letters feature.");
      return;
    }
    const found = await session.checkCompatibility();
    const question = switchQuestion(found);
    if (!question) session.announce("Strict Word mode: only what Word keeps exactly.");
    else if (found.some((issue) => issue.fix)) session.say(question, { label: "Fix All", run: () => session.fixIssues() });
    else session.say(question, { label: "Show", run: () => (compatibilityOpen = true) });
  }

  // what Word would show differently is asked again when edits settle (Strict mode)
  let compatibilityTimer: ReturnType<typeof setTimeout> | null = null;
  $effect(() => {
    void editor.doc;
    if (mode !== "strict") return;
    if (compatibilityTimer) clearTimeout(compatibilityTimer);
    compatibilityTimer = setTimeout(() => session.checkCompatibility(), 800);
  });

  onMount(() => {
    api.wordAvailable().then((available) => (wordInstalled = available));
    const ticking = setInterval(() => (clock = Date.now()), 15000);
    return () => clearInterval(ticking);
  });

  // Word (Strict mode) or the engine (a setting) lay the document out in the
  // background after edits pause; typing never waits for them
  $effect(() => {
    layoutCheck.documentChanged(provider, editor.doc);
  });

  async function checkLayout() {
    if (provider === "word") await layoutCheck.checkWord();
  }

  const shownLayout = $derived(layoutCheck.shown(provider));
  const layoutStale = $derived(layoutCheck.isStale(provider, editor.doc));

  // read by the end-to-end scripts (nothing in the app reads it)
  onMount(() => {
    const handle = { session: () => session, wordLayout: () => layoutCheck.word, lastCheckMs: () => layoutCheck.lastCheckMs };
    (window as unknown as { __lettersWriter?: typeof handle }).__lettersWriter = handle;
    if (opened.convertedFrom) session.say(`Opened “${opened.convertedFrom}” by converting it. Save writes “${session.name}” beside it and leaves the original as it was.`);
  });

  // the session turns screen sizes into page sizes with the zoom
  $effect(() => {
    session.zoom = zoom;
  });

  // --- google drive

  let driveOpen = $state(false);

  // --- commands

  function writerCommand(name: WriterCommand) {
    if (session.command(name)) return;
    switch (name) {
      case "insert-table":
        // the grid picker lives in the Insert menu; from a key or the menu bar, a 2 by 2 table
        session.insertTable(2, 2);
        break;
      case "insert-picture":
        session.insertPicture();
        break;
      case "insert-link":
        prompt = "link";
        break;
      case "insert-comment":
        if (editor.selection.empty) session.say("Select the words the comment is about first.");
        else prompt = "comment";
        break;
      case "find-replace":
        openFind(true);
        break;
      case "export-pdf":
        exportPdf();
        break;
      case "toggle-ruler":
        rulerShown = !rulerShown;
        break;
      case "print":
        print();
        break;
      case "save-to-drive":
        driveOpen = true;
        break;
      case "toggle-strict-mode":
        setMode(mode === "strict" ? "free" : "strict");
        break;
      case "image-inline":
        session.objects.setImageFree(false);
        break;
      case "image-free":
        session.objects.setImageFree(true);
        break;
      case "image-crop":
        objectFrame?.toggleCrop();
        break;
      case "text-block-split":
        session.objects.split();
        break;
      case "text-block-make":
        session.objects.make();
        break;
      case "text-block-dissolve":
        session.objects.dissolve();
        break;
      case "object-front":
        session.objects.stack(true);
        break;
      case "object-back":
        session.objects.stack(false);
        break;
      case "toggle-focus-mode":
        focusMode = !focusMode;
        session.announce(focusMode ? "Focus Mode: only the page. Press Ctrl Shift Enter to leave." : "Focus Mode is off.");
        break;
      case "check-layout":
        if (provider === "word") checkLayout();
        else session.say(wordInstalled ? "Turn on Strict Word mode to have Word lay out the pages." : "Word isn’t installed on this computer, so pages are estimated.");
        break;
    }
  }

  function openFind(withReplace = false) {
    if (findOpen) findBar?.focus(withReplace);
    else {
      replaceShown = withReplace;
      findOpen = true;
    }
  }

  /** Replace writes over the other change, Save As keeps it, Cancel leaves both as they are. */
  function answerChangedOnDisk(choice: ChangedChoice) {
    session.changedOnDisk = null;
    if (choice === "replace") session.save(null, true);
    else if (choice === "save-as") saveAs();
  }

  /** Save: a document converted from another file has no place of its own yet, so it asks where. */
  async function saveNow(): Promise<boolean> {
    if (session.neverSaved) return saveAs();
    return session.save();
  }

  function onToolbarCommand(command: WriterCommand | "save" | "save-as" | "undo" | "redo") {
    if (command === "save") saveNow();
    else if (command === "save-as") saveAs();
    else if (command === "undo") session.undo();
    else if (command === "redo") session.redo();
    else writerCommand(command);
  }

  $effect(() => {
    const handlers: [Command, () => void][] = [
      ["save", () => session.dirty && saveNow()],
      ["save-as", saveAs],
      ["undo", () => session.undo()],
      ["redo", () => session.redo()],
      ["find", () => openFind(false)],
      ["zoom-in", () => setZoom(zoomIn(zoom))],
      ["zoom-out", () => setZoom(zoomOut(zoom))],
      ["fit-width", fitWidth],
      ["toggle-inspector", () => toggle("inspector")],
      ["toggle-night-mode", () => (nightMode = !nightMode)],
      ...WRITER_COMMANDS.map((name): [Command, () => void] => [name, () => writerCommand(name)]),
    ];
    const takeBack = handlers.map(([command, handler]) => appCommands.register(command, handler));
    return () => takeBack.forEach((unregister) => unregister());
  });

  /** the writer's keys are matched first (see shortcuts.ts), anywhere in the window. */
  function onKeyCapture(event: KeyboardEvent) {
    const command = writerCommandForKey(keyPressOf(event));
    if (!command) return;
    event.preventDefault();
    event.stopPropagation();
    appCommands.dispatch(command, "keyboard");
  }

  function onWindowKey(event: KeyboardEvent) {
    if (event.key !== "Escape" || event.defaultPrevented) return;
    if (prompt) prompt = null;
    else if (findOpen) closeFind();
  }

  function closeFind() {
    findOpen = false;
    session.view?.focus();
  }

  // --- leaving, the title, autosave

  onMount(() => {
    writerGuard.mayLeave = (ask) =>
      writerMayLeave({ name: session.name, dirty: session.dirty }, ask, saveNow, () => session.discardRecovery());
    window.addEventListener("keydown", onKeyCapture, true);
    const timer = setInterval(() => session.autosave(), 5000);
    tick().then(fitWidth);
    // a search hit lands on its paragraph with its words in Find; a moved file is followed (neighbours.ts)
    tick().then(() => arriveAt(session, (query) => ((findQuery = query), openFind(false))));
    const stopFollowing = followMoves(session);
    goals.start(session.path, () => session.counts().words);
    return () => {
      clearInterval(timer);
      window.removeEventListener("keydown", onKeyCapture, true);
      stopFollowing();
      goals.stop(session.counts().words);
    };
  });

  onDestroy(() => {
    writerGuard.mayLeave = null;
    api.closeWriterDocument(session.id).catch(() => {});
    getCurrentWindow().setTitle("Recents");
  });

  $effect(() => {
    getCurrentWindow().setTitle(dirty ? `${session.name} — Edited` : session.name);
  });

  // the outline of a wide window shows at the start, like Pages' sidebar
  onMount(() => {
    if (window.innerWidth > 1180) panels = openPanel(panels, "sidebar", window.innerWidth, remPx);
  });

  $effect(() => {
    const width = windowWidth;
    untrack(() => {
      const fitted = fitPanels(panels, width, remPx);
      if (fitted.length !== panels.length) panels = fitted;
    });
  });

  // the page follows the window's width until the reader picks a zoom
  $effect(() => {
    void viewportWidth;
    void page.width;
    untrack(() => {
      if (followsWidth && viewportWidth > 0) fitWidth();
    });
  });

  // the engine's sentences about the ruler, for Explain, when it is open
  let rulerTimer: ReturnType<typeof setTimeout> | null = null;
  $effect(() => {
    if (!showExplain) return;
    const index = editor.selection.$from.index(0);
    void editor.doc;
    if (rulerTimer) clearTimeout(rulerTimer);
    rulerTimer = setTimeout(() => {
      api.describeRuler(session.model(), index).then((ruler) => (rulerSentences = ruler.description)).catch(() => (rulerSentences = []));
    }, 300);
  });

  // --- the recovered version

  async function recover() {
    try {
      const fromFile = modelToDoc(opened.model);
      const recovered = await api.openWriterDocument(session.path, true);
      await api.closeWriterDocument(session.id);
      const next = new WriterSession(recovered, onError);
      // the file on disk still has the old words, so the recovered ones count as unsaved
      next.saved = fromFile;
      session = next;
      recovery = null;
      next.say("The recovered version is open. Save to keep it.");
    } catch (problem) {
      onError(errorText(problem));
    }
  }

  function dismissRecovery() {
    session.discardRecovery();
    recovery = null;
  }

  // clicking commented text shows the comment in Review
  function onPageClick(event: MouseEvent) {
    const commented = (event.target as HTMLElement).closest?.(".w-comment") as HTMLElement | null;
    if (commented?.dataset.comment) {
      focusComment = commented.dataset.comment;
      openInspector("review");
    }
  }
</script>

<svelte:window bind:innerWidth={windowWidth} onkeydown={onWindowKey} onresize={() => (remPx = remInPixels())} />

<div class="writer" class:focus-mode={focusMode}>
  {#if showSidebar}
    <WriterOutline {session} />
  {/if}

  <!-- the document and its controls are the page's main landmark -->
  <main class="stage" aria-label={session.name}>
    <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
    <div class="scroller" bind:clientWidth={viewportWidth} style:padding-top="{toolbarHeight}px" onclick={onPageClick}>
      {#key session.id}
        {#if rulerShown && current && !focusMode}
          <div class="ruler-band">
            <Ruler
              frame={{ pageWidth: page.width, marginLeft: page.margins.left, marginRight: page.margins.right }}
              indent={current.props.indent ?? {}}
              tabs={(current.props.tabs ?? []).filter((stop) => stop.kind !== "clear")}
              {zoom}
              {unit}
              onIndent={(indent) => session.run((s) => edit.setIndent(s, () => indent), false)}
              onTabs={(tabs) => session.setTabStops(tabs)}
            />
          </div>
        {/if}
        <PageSurface {session} {zoom} {nightMode} {focusMode} layout={{ provider, word: shownLayout }} bind:pageCount bind:currentPage />
        <ObjectFrame bind:this={objectFrame} {session} {zoom} {unit} />
      {/key}
    </div>

    <WriterToolbar
      name={session.name}
      edited={dirty}
      canUndo={session.canUndo}
      canRedo={session.canRedo}
      {zoom}
      {narrow}
      {styleKey}
      styles={meta.styles}
      sidebarOpen={showSidebar}
      {findOpen}
      inspectorOpen={showInspector}
      explainOpen={showExplain}
      {nightMode}
      {rulerShown}
      bind:height={toolbarHeight}
      onCommand={onToolbarCommand}
      onStyle={(key) => session.applyStyle(key)}
      onInsertTable={(rows, columns) => session.insertTable(rows, columns)}
      onSidebar={() => toggle("sidebar")}
      {onRecents}
      onZoomOut={() => setZoom(zoomOut(zoom))}
      onZoomIn={() => setZoom(zoomIn(zoom))}
      onFitWidth={fitWidth}
      onFind={() => (findOpen ? closeFind() : openFind())}
      onNightMode={() => (nightMode = !nightMode)}
      onRuler={() => (rulerShown = !rulerShown)}
      {onNewWindow}
      onInspector={() => toggle("inspector")}
      onExplain={() => toggle("explain")}
    />

    {#if findOpen}
      <FindReplace bind:this={findBar} {session} top={toolbarHeight} initialQuery={findQuery} bind:replaceShown onClose={closeFind} />
    {/if}

    <WriterNotices {session} top={toolbarHeight} {recovery} onRecover={recover} onDismissRecovery={dismissRecovery} bind:odtQuestion bind:prompt />

    {#if session.changedOnDisk}
      <!-- the file changed elsewhere since it was opened: the same question the pdf viewer asks -->
      <ChangedOnDiskSheet name={session.name} message={session.changedOnDisk} onChoose={answerChangedOnDisk} />
    {/if}

    <StatusBar
      goals={goals.line(counts.words)}
      words={counts.words}
      selected={counts.selected}
      page={currentPage}
      pages={pageCount}
      language={session.language()}
      settings={session.settings}
      tracking={session.tracking}
      onSettings={(next) => session.setSettings(next)}
      onTracking={() => openInspector("review")}
      {mode}
      onMode={setMode}
      layoutLabel={providerLabel(provider, { checking: layoutCheck.checking, checkedAt: layoutCheck.checkedAt, wordInstalled, stale: layoutStale, failed: layoutCheck.failed }, clock)}
      layoutTooltip={`${providerExplanation(provider, wordInstalled)}${layoutCheck.lastCheckMs !== null ? ` The last check took ${(layoutCheck.lastCheckMs / 1000).toFixed(1)} s.` : ""}`}
      onCheckLayout={provider === "word" && !layoutCheck.checking ? checkLayout : null}
      compatibility={mode === "strict" ? compatibilityLine(issues) : null}
      compatibilityDetails={issues.map((issue) => issue.message).join(" ")}
      onShowCompatibility={() => (compatibilityOpen = true)}
      missingFonts={missing}
      onShowMissingFonts={() => openFontLibrary({ documentFonts: usedFonts, onReplaceFont, view: "missing" })}
    />

    {#if compatibilityOpen}
      <CompatibilitySheet {session} top={toolbarHeight} onClose={() => (compatibilityOpen = false)} />
    {/if}

    {#if driveOpen}
      <DriveSheet {session} top={toolbarHeight} onClose={() => (driveOpen = false)} />
    {/if}

    <div class="visually-hidden" role="status" aria-live="polite">{session.announcement}</div>
  </main>

  {#if showInspector}
    <WriterInspector
      {session}
      tab={inspectorTab}
      {unit}
      floating={floatingInspector}
      {focusComment}
      onTab={(tab) => (inspectorTab = tab)}
      onClose={() => {
        panels = closePanel(panels, "inspector");
        session.view?.focus();
      }}
    />
  {/if}

  {#if showExplain}
    <WriterExplain groups={explainGroups} ruler={rulerSentences} onClose={() => (panels = closePanel(panels, "explain"))} />
  {/if}
</div>

<style>
  .writer {
    position: relative;
    display: flex;
    height: 100%;
  }

  /* Focus Mode: the controls fade until the pointer comes near them */
  .writer.focus-mode :global(.toolbar),
  .writer.focus-mode :global(footer.status) {
    opacity: 0.15;
    transition: opacity 0.2s var(--ease);
  }

  .writer.focus-mode :global(.toolbar:hover),
  .writer.focus-mode :global(footer.status:hover),
  .writer.focus-mode :global(.toolbar:focus-within) {
    opacity: 1;
  }

  .stage {
    position: relative;
    flex: 1;
    min-width: 0;
  }

  .scroller {
    box-sizing: border-box;
    width: 100%;
    height: 100%;
    overflow: auto;
    padding-bottom: 3rem;
    background: var(--desk);
  }

  /* the ruler stays under the toolbar while the pages scroll */
  .ruler-band {
    position: sticky;
    /* sticky offsets count from inside the padding, which already clears the toolbar */
    top: 0;
    z-index: 1;
    min-width: max-content;
    background: linear-gradient(to bottom, var(--desk) 70%, transparent);
  }

  /* printing (the export fallback) shows only the pages */
  @media print {
    .writer > :global(:not(.stage)),
    .stage > :global(:not(.scroller)),
    .ruler-band {
      display: none !important;
    }
    .scroller {
      overflow: visible;
      height: auto;
      padding: 0 !important;
      background: #ffffff;
    }
    .writer,
    .stage {
      display: block;
      height: auto;
    }
  }
</style>

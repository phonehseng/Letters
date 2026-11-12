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
  import { tools } from "../tools";
  import { modelToDoc } from "./convert";
  import * as edit from "./editing";
  import { writerExplainGroups, type WriterTab } from "./explainList";
  import FindReplace from "./FindReplace.svelte";
  import type { DocumentMeta } from "./model";
  import PageSurface from "./PageSurface.svelte";
  import PromptSheet from "./PromptSheet.svelte";
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
  import ImageFrame from "./ImageFrame.svelte";
  import { formatForPath, saveFilters } from "./formats";
  import { chooseProvider, providerExplanation, providerLabel, type EditingMode, type WordLayout } from "./layoutSource";
  import { printPdfFile } from "./print";
  import { documentFonts, missingFonts } from "./fonts";
  import { compatibilityIssues, compatibilityLine } from "./strictMode";

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

  async function saveAs(): Promise<boolean> {
    const path = await api.chooseSaveLocationWithFilters(session.path, saveFilters(session.kind));
    if (!path) return false;
    const format = formatForPath(path);
    if (!format) {
      onError("Letters can’t save in that format yet. Choose Word, OpenDocument, PDF, Web Page or Plain Text.");
      return false;
    }
    if (format.id === "pdf") {
      await session.exportPdf(path);
      return true;
    }
    if (format.becomes === "copy") {
      try {
        await api.exportCopy(session.id, session.model(), path);
        session.say(`Saved a copy as ${format.label}. ${format.note} This document is still “${session.name}”.`);
        return true;
      } catch (problem) {
        onError(errorText(problem));
        return false;
      }
    }
    if (/\.odt$/i.test(path)) {
      const losses = await api.odtLosses(session.id, session.model()).catch(() => null);
      if (losses) {
        odtQuestion = { text: losses, path };
        return false;
      }
    }
    return session.save(path);
  }

  async function exportPdf() {
    const path = await api.chooseSaveLocationFor(api.withExtension(session.path, "pdf"), "pdf");
    if (path) await session.exportPdf(path);
  }

  /** prints Word's (or LibreOffice's) print-ready pdf, or the surface itself without them. */
  async function print() {
    session.say("Preparing to print…");
    try {
      const pdf = await api.printReadyPdf(session.id, session.model());
      session.notice = null;
      await printPdfFile(pdf);
    } catch {
      session.say("Word and LibreOffice aren’t installed, so Letters prints its own pages. Page breaks may differ from Word’s.");
      setTimeout(() => window.print(), 300);
    }
  }

  // --- editing mode and where page breaks come from (layoutSource.ts)

  let wordInstalled = $state(false);
  let wordLayoutResult = $state<WordLayout | null>(null);
  let layoutChecking = $state(false);
  let layoutCheckedAt = $state<number | null>(null);
  let clock = $state(Date.now());
  const mode = $derived<EditingMode>(session.settings.mode === "strict" ? "strict" : "free");
  const provider = $derived(chooseProvider(mode, { word: wordInstalled, engine: false }));
  const missing = $derived(missingFonts(documentFonts(meta)));
  const issues = $derived(mode === "strict" ? compatibilityIssues(session.model(), opened.kind === "odt") : []);

  function setMode(next: EditingMode) {
    session.setSettings({ ...session.settings, mode: next });
    session.announce(next === "strict" ? "Strict Word mode: only what Word keeps exactly." : "Free mode: every Letters feature.");
  }

  onMount(() => {
    api.wordAvailable().then((available) => (wordInstalled = available));
    const ticking = setInterval(() => (clock = Date.now()), 15000);
    return () => clearInterval(ticking);
  });

  // in Strict Word mode Word lays the document out after edits settle, in the background
  let layoutTimer: ReturnType<typeof setTimeout> | null = null;
  let laidOutDoc: unknown = null;
  $effect(() => {
    const doc = editor.doc;
    if (provider !== "word" || doc === laidOutDoc) return;
    if (layoutTimer) clearTimeout(layoutTimer);
    layoutTimer = setTimeout(async () => {
      laidOutDoc = doc;
      layoutChecking = true;
      try {
        wordLayoutResult = await api.wordLayout(session.id, session.model());
        layoutCheckedAt = Date.now();
      } catch {
        wordLayoutResult = null;
      } finally {
        layoutChecking = false;
      }
    }, 2500);
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
        session.setImageFree(false);
        break;
      case "image-free":
        session.setImageFree(true);
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

  function onToolbarCommand(command: WriterCommand | "save" | "save-as" | "undo" | "redo") {
    if (command === "save") session.save();
    else if (command === "save-as") saveAs();
    else if (command === "undo") session.undo();
    else if (command === "redo") session.redo();
    else writerCommand(command);
  }

  $effect(() => {
    const handlers: [Command, () => void][] = [
      ["save", () => session.dirty && session.save()],
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
      writerMayLeave({ name: session.name, dirty: session.dirty }, ask, () => session.save(), () => session.discardRecovery());
    window.addEventListener("keydown", onKeyCapture, true);
    const timer = setInterval(() => session.autosave(), 5000);
    tick().then(fitWidth);
    return () => {
      clearInterval(timer);
      window.removeEventListener("keydown", onKeyCapture, true);
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

<div class="writer">
  {#if showSidebar}
    <WriterOutline {session} />
  {/if}

  <div class="stage">
    <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
    <div class="scroller" bind:clientWidth={viewportWidth} style:padding-top="{toolbarHeight}px" onclick={onPageClick}>
      {#key session.id}
        {#if rulerShown && current}
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
        <PageSurface {session} {zoom} {nightMode} layout={{ provider, word: wordLayoutResult }} bind:pageCount bind:currentPage />
        <ImageFrame {session} {zoom} {unit} />
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
      <FindReplace bind:this={findBar} {session} top={toolbarHeight} bind:replaceShown onClose={closeFind} />
    {/if}

    {#if recovery}
      <div class="glass notice" role="alertdialog" aria-label="Recovered version" style:top="{toolbarHeight + 8}px">
        <span>Letters kept unsaved changes to this document from {new Date(recovery.savedAt * 1000).toLocaleString()}, when it last closed without saving.</span>
        <button class="button" onclick={dismissRecovery}>Discard</button>
        <button class="button primary" onclick={recover}>Recover</button>
      </div>
    {:else if session.notice}
      <div class="glass notice" role="status" style:top="{toolbarHeight + 8}px">
        <span>{session.notice.text}</span>
        {#if session.notice.action}
          <button
            class="button"
            onclick={() => {
              const action = session.notice?.action;
              session.notice = null;
              action?.run();
            }}>{session.notice.action.label}</button
          >
        {/if}
        <button class="button" onclick={() => (session.notice = null)}>OK</button>
      </div>
    {/if}

    {#if odtQuestion}
      <div class="glass notice" role="alertdialog" aria-label="Save as OpenDocument" style:top="{toolbarHeight + 8}px">
        <span>{odtQuestion.text}</span>
        <button class="button" onclick={() => (odtQuestion = null)}>Cancel</button>
        <button
          class="button primary"
          onclick={() => {
            const path = odtQuestion?.path;
            odtQuestion = null;
            if (path) session.save(path);
          }}>Save as ODT</button
        >
      </div>
    {/if}

    {#if prompt === "link"}
      <PromptSheet
        title={tools.wInsertLink.label.replace("…", "")}
        label="Web address"
        placeholder="https://"
        value={String(edit.markData(editor, editor.schema.marks.link)?.href ?? "")}
        confirm="Add Link"
        top={toolbarHeight}
        secondary={edit.markActive(editor, editor.schema.marks.link) ? { label: "Remove Link", run: () => ((prompt = null), session.setLink(null)) } : undefined}
        onSubmit={(href) => {
          prompt = null;
          session.setLink(/^[a-z]+:/i.test(href) ? href : `https://${href}`);
          session.view?.focus();
        }}
        onCancel={() => ((prompt = null), session.view?.focus())}
      />
    {:else if prompt === "comment"}
      <PromptSheet
        title="Comment"
        label="Your comment on the selected text"
        confirm="Add Comment"
        multiline
        top={toolbarHeight}
        onSubmit={(text) => {
          prompt = null;
          session.addComment(text);
          session.view?.focus();
        }}
        onCancel={() => ((prompt = null), session.view?.focus())}
      />
    {/if}

    <StatusBar
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
      layoutLabel={providerLabel(provider, { checking: layoutChecking, checkedAt: layoutCheckedAt, wordInstalled }, clock)}
      layoutTooltip={providerExplanation(provider, wordInstalled)}
      compatibility={mode === "strict" ? compatibilityLine(issues) : null}
      compatibilityDetails={issues.map((issue) => issue.what).join("; ")}
      missingFonts={missing}
    />

    {#if driveOpen}
      <DriveSheet {session} top={toolbarHeight} onClose={() => (driveOpen = false)} />
    {/if}

    <div class="visually-hidden" role="status" aria-live="polite">{session.announcement}</div>
  </div>

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

  .notice {
    position: absolute;
    left: 50%;
    z-index: 3;
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 0.75rem;
    max-width: min(36rem, calc(100% - 2rem));
    padding: 0.625rem 0.625rem 0.625rem 1rem;
    transform: translateX(-50%);
    animation: arrive 0.2s var(--ease);
  }

  .notice span {
    flex: 1 1 14rem;
  }

  @keyframes arrive {
    from {
      opacity: 0;
      transform: translate(-50%, -0.5rem);
    }
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

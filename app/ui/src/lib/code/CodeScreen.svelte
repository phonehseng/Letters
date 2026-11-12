<script lang="ts">
  // the code editor screen: one code or text file per window. the editor
  // is the content; the toolbar, the status bar and the Problems panel
  // float over the desk as everywhere else. checks run a moment after
  // typing pauses and on save (scheduler.ts); nothing is ever run.
  import { redo, toggleComment, undo } from "@codemirror/commands";
  import { gotoLine, openSearchPanel } from "@codemirror/search";
  import type { Text } from "@codemirror/state";
  import type { EditorView } from "@codemirror/view";
  import { getCurrentWindow } from "@tauri-apps/api/window";
  import { onDestroy, onMount, untrack } from "svelte";
  import { appCommands, type Command } from "../commands";
  import { writerGuard, writerMayLeave } from "../writer/writerGuard";
  import { checkInApp, inAppCoverage } from "./checks";
  import CodeEditor from "./CodeEditor.svelte";
  import CodeExplain from "./CodeExplain.svelte";
  import { cancelCompilerCheck, codeTools as findCodeTools, openInVsCode, revealInFolder, type CodeDocument } from "./codeApi";
  import type { CodeCommand } from "./codeCommands";
  import { CodeSession, AUTOSAVE_INTERVAL_MS, errorText } from "./codeSession.svelte";
  import CodeStatusBar from "./CodeStatusBar.svelte";
  import CodeToolbar from "./CodeToolbar.svelte";
  import { checkerLine, hasCompilerCheck, problemsToShow, runCompilerCheck, type CompilerResult } from "./compilerCheck";
  import { goTo, showProblems } from "./editorSetup";
  import { languageInfo, type LanguageId } from "./languages";
  import { nextProblem, type Problem } from "./problems";
  import ProblemsPanel from "./ProblemsPanel.svelte";
  import { CheckScheduler, COMPILER_PAUSE_MS, IN_APP_PAUSE_MS } from "./scheduler";
  import { grammarProblems } from "./syntaxErrors";
  import { detectIndentation, largeFileNotice } from "./textFormat";
  import { hasBasicCheck } from "./checks/balance";

  interface Props {
    document: CodeDocument;
    onError: (message: string) => void;
    onRecents: () => void;
    onNewWindow: () => void;
  }

  let { document: opened, onError, onRecents, onNewWindow }: Props = $props();

  const session = untrack(() => new CodeSession(opened, onError));
  let view = $state<EditorView>();
  let language = $state<LanguageId>(untrack(() => session.startingLanguage(opened)));
  let wrap = $state(untrack(() => language === "markdown" || language === "text"));
  let indentation = $state(untrack(() => detectIndentation(opened.text)));
  let cursor = $state({ line: 1, column: 1 });
  let inAppProblems = $state<Problem[]>([]);
  let compiler = $state<CompilerResult | null>(null);
  let checking = $state(false);
  let problemsOpen = $state(false);
  let explainOpen = $state(false);
  let vscode = $state(false);
  let fontSize = $state(13);
  let windowWidth = $state(window.innerWidth);
  let recovery = $state(untrack(() => opened.recovery));

  const problems = $derived(problemsToShow(inAppProblems, compiler));
  const coverage = $derived(inAppCoverage(language));
  const checkedBy = $derived(session.fullFeatures ? checkerLine(language, compiler) : "Not checked: the file is large");
  const reducedMotion = window.matchMedia("(prefers-reduced-motion: reduce)").matches;

  // --- checks

  function grammarIfUseful(): Problem[] {
    // the grammar's error marks only help where Letters has no parser of its own
    if (!view || !hasBasicCheck(language) || language === "python") return [];
    return grammarProblems(view.state);
  }

  const inAppChecks = new CheckScheduler(
    async () => {
      const text = view?.state.doc.toString() ?? "";
      return [...(await checkInApp(language, text)), ...grammarIfUseful()];
    },
    (found) => (inAppProblems = found),
    IN_APP_PAUSE_MS,
  );

  const compilerChecks = new CheckScheduler(
    async () => {
      checking = true;
      try {
        return await runCompilerCheck(language, view?.state.doc.toString() ?? "", session.path);
      } finally {
        checking = false;
      }
    },
    (result) => (compiler = result),
    COMPILER_PAUSE_MS,
    () => void cancelCompilerCheck().catch(() => {}),
  );

  function checkSoon() {
    if (!session.fullFeatures) return;
    inAppChecks.changed();
    if (hasCompilerCheck(language)) compilerChecks.changed();
  }

  function checkNow() {
    if (!session.fullFeatures) return;
    void inAppChecks.now();
    if (hasCompilerCheck(language)) void compilerChecks.now();
    else compiler = null;
  }

  $effect(() => {
    if (view) showProblems(view, problems);
  });

  // --- the editor's events

  const events = {
    onChange: (doc: Text) => {
      session.changed(doc);
      checkSoon();
    },
    onCursor: (line: number, column: number) => (cursor = { line, column }),
    onToggleWrap: () => (wrap = !wrap),
    onToggleProblems: () => (problemsOpen = !problemsOpen),
    onNextProblem: (backwards: boolean) => {
      const next = nextProblem(problems, cursor.line, backwards);
      if (next && view) goTo(view, next.line, next.column);
    },
  };

  function chooseLanguage(next: LanguageId) {
    language = next;
    compiler = null;
    checkNow();
  }

  // --- saving

  async function save() {
    if (session.readOnly) return;
    if (await session.save()) checkNow();
  }

  async function saveAs() {
    if (await session.saveAs()) checkNow();
  }

  // --- commands

  function run(command: (target: EditorView) => boolean) {
    if (view) command(view);
    view?.focus();
  }

  function codeCommand(name: CodeCommand) {
    switch (name) {
      case "code-go-to-line":
        return run(gotoLine);
      case "code-toggle-comment":
        return run(toggleComment);
      case "code-toggle-wrap":
        return events.onToggleWrap();
      case "code-toggle-problems":
        return events.onToggleProblems();
      case "code-next-problem":
        return events.onNextProblem(false);
      case "code-check-now":
        return checkNow();
      case "code-open-in-vscode":
        return openVsCode();
      case "code-reveal":
        return reveal();
    }
  }

  async function openVsCode() {
    if (!session.path || !vscode) return;
    if (session.dirty) await save();
    openInVsCode(session.path, cursor.line, cursor.column).catch((problem) => onError(errorText(problem)));
  }

  function reveal() {
    if (session.path) revealInFolder(session.path).catch((problem) => onError(errorText(problem)));
  }

  const CODE_SCREEN_COMMANDS: CodeCommand[] = ["code-go-to-line", "code-toggle-comment", "code-toggle-wrap", "code-toggle-problems", "code-next-problem", "code-check-now", "code-open-in-vscode", "code-reveal"];

  $effect(() => {
    const handlers: [Command, () => void][] = [
      ["save", () => void save()],
      ["save-as", () => void saveAs()],
      ["undo", () => run(undo)],
      ["redo", () => run(redo)],
      ["find", () => run(openSearchPanel)],
      ["zoom-in", () => (fontSize = Math.min(32, fontSize + 1))],
      ["zoom-out", () => (fontSize = Math.max(9, fontSize - 1))],
      ["fit-width", () => (fontSize = 13)],
      ...CODE_SCREEN_COMMANDS.map((name): [Command, () => void] => [name, () => codeCommand(name)]),
    ];
    const takeBack = handlers.map(([command, handler]) => appCommands.register(command, handler));
    return () => takeBack.forEach((unregister) => unregister());
  });

  // --- leaving, the title, recovery copies

  onMount(() => {
    writerGuard.mayLeave = (ask) => writerMayLeave({ name: session.name, dirty: session.dirty }, ask, () => session.save(), () => session.discardRecovery());
    const timer = setInterval(() => void session.autosave(), AUTOSAVE_INTERVAL_MS);
    findCodeTools().then((found) => (vscode = found.vscode)).catch(() => (vscode = false));
    const notice = largeFileNotice(opened.bytes, session.readOnly);
    if (notice) session.notice = { text: notice };
    else if (session.readOnly) session.notice = { text: `“${session.name}” is read-only, so it opens for reading. Use Save As to keep a copy you can change.` };
    return () => clearInterval(timer);
  });

  // the first text, once the editor exists; a new untitled block counts as unsaved
  $effect(() => {
    if (!view) return;
    untrack(() => {
      session.started(view!.state.doc, session.path !== null);
      checkNow();
    });
  });

  onDestroy(() => {
    inAppChecks.stop();
    compilerChecks.stop();
    writerGuard.mayLeave = null;
    getCurrentWindow().setTitle("Recents");
  });

  $effect(() => {
    getCurrentWindow().setTitle(session.dirty ? `${session.name} — Edited` : session.name);
  });

  function recover() {
    const copy = recovery;
    if (!copy || !view) return;
    view.dispatch({ changes: { from: 0, to: view.state.doc.length, insert: copy.text } });
    recovery = null;
    session.notice = { text: "The recovered version is open. Save to keep it." };
  }

  function dismissRecovery() {
    void session.discardRecovery();
    recovery = null;
  }

  // read by the end-to-end scripts (nothing in the app reads it)
  onMount(() => {
    const handle = { view: () => view, problems: () => problems, checkedBy: () => checkedBy, session: () => session, language: () => language };
    (window as unknown as { __lettersCode?: typeof handle }).__lettersCode = handle;
  });
</script>

<svelte:window bind:innerWidth={windowWidth} />

<div class="code-screen" style:--code-size="{fontSize}px">
  <div class="main">
    <CodeToolbar
      name={session.name}
      edited={session.dirty}
      languageLabel={languageInfo(language).label}
      errorCount={problems.filter((problem) => problem.severity === "error").length}
      {problemsOpen}
      {explainOpen}
      canSave={!session.readOnly}
      {vscode}
      hasFile={session.path !== null}
      narrow={windowWidth < 760}
      {onRecents}
      onSave={() => void save()}
      onFind={() => run(openSearchPanel)}
      onGoToLine={() => run(gotoLine)}
      onComment={() => run(toggleComment)}
      onProblems={events.onToggleProblems}
      onVsCode={() => void openVsCode()}
      onReveal={reveal}
      {onNewWindow}
      onExplain={() => (explainOpen = !explainOpen)}
    />

    {#if recovery}
      <div class="glass notice" role="alertdialog" aria-label="Recovered version">
        <span>Letters kept unsaved changes to this file from {new Date(recovery.savedAt * 1000).toLocaleString()}, when it last closed without saving.</span>
        <button class="button" onclick={dismissRecovery}>Discard</button>
        <button class="button primary" onclick={recover}>Recover</button>
      </div>
    {:else if session.notice}
      <div class="glass notice" role="status">
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

    <div class="paper">
      <CodeEditor
        options={{ text: opened.text, fullFeatures: session.fullFeatures, readOnly: session.readOnly, wrap, indentation, label: `Code editor, ${session.name}`, reducedMotion }}
        {events}
        {language}
        {wrap}
        {indentation}
        bind:view
      />
    </div>

    {#if problemsOpen}
      <ProblemsPanel
        {problems}
        checkerLine={checkedBy}
        {coverage}
        {checking}
        onGoTo={(problem) => view && goTo(view, problem.line, problem.column)}
        onCheckNow={checkNow}
        onClose={() => {
          problemsOpen = false;
          view?.focus();
        }}
      />
    {/if}

    <CodeStatusBar
      line={cursor.line}
      column={cursor.column}
      {problems}
      checkerLine={checking ? "Checking…" : checkedBy}
      {coverage}
      {language}
      {indentation}
      format={session.format}
      {wrap}
      onLanguage={chooseLanguage}
      onIndentation={(next) => (indentation = next)}
      onWrap={events.onToggleWrap}
      onProblems={events.onToggleProblems}
    />
  </div>

  {#if explainOpen}
    <CodeExplain {coverage} checkerLine={checkedBy} {vscode} onClose={() => (explainOpen = false)} />
  {/if}
</div>

<style>
  .code-screen {
    display: flex;
    height: 100%;
    background: var(--desk);
  }

  .main {
    display: flex;
    flex: 1;
    flex-direction: column;
    min-width: 0;
  }

  /* the code is the content: it fills the desk between the floating bars */
  .paper {
    flex: 1;
    min-height: 0;
    margin: 0 0.5rem 0.5rem;
    overflow: hidden;
    border-radius: var(--radius-group);
    box-shadow: var(--shadow-page);
  }

  .notice {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 0.75rem;
    margin: 0 auto 0.5rem;
    max-width: min(36rem, calc(100% - 2rem));
    padding: 0.625rem 0.625rem 0.625rem 1rem;
  }

  .notice span {
    flex: 1 1 14rem;
  }
</style>

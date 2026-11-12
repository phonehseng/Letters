<script lang="ts">
  // the slide editor: one presentation per window. the slide is the
  // content on the desk; the toolbar, the slide rail, the notes and the
  // inspector float as glass around it. every edit is one of the engine's
  // operations, sent to the shell (deckSession.svelte.ts); the slide show
  // takes the whole window, or the screens, while it runs.
  import { getCurrentWindow } from "@tauri-apps/api/window";
  import { onDestroy, onMount, tick, untrack } from "svelte";
  import { pictureSize } from "../annotate/annotateApi";
  import { appCommands, isTextField, type Command } from "../commands";
  import { writerGuard, writerMayLeave } from "../writer/writerGuard";
  import { AUTOSAVE_INTERVAL_MS, DeckSession } from "./deckSession.svelte";
  import ExportSheet from "./ExportSheet.svelte";
  import { centredBox, fitScale } from "./geometry";
  import NotesPane from "./NotesPane.svelte";
  import { newSlideLayout } from "./railOrder";
  import SlideCanvas from "./SlideCanvas.svelte";
  import SlideRail from "./SlideRail.svelte";
  import SlideShow from "./SlideShow.svelte";
  import { isMac, SLIDES_COMMANDS, slidesCommandForKey, type SlidesCommand } from "./slidesCommands";
  import SlidesExplain from "./SlidesExplain.svelte";
  import SlidesFind from "./SlidesFind.svelte";
  import SlidesInspector, { type SlidesTab } from "./SlidesInspector.svelte";
  import SlidesToolbar, { type ToolbarAction } from "./SlidesToolbar.svelte";
  import { choosePictureFile, closeDeck, openDeck, type OpenedDeck } from "./slidesApi";

  interface Props {
    opened: OpenedDeck;
    onError: (message: string) => void;
    onRecents: () => void;
    onNewWindow: () => void;
  }

  let { opened, onError, onRecents, onNewWindow }: Props = $props();

  let session = $state(untrack(() => new DeckSession(opened, onError)));
  let recovery = $state(untrack(() => opened.recovery));
  let railOpen = $state(true);
  let notesOpen = $state(true);
  let inspectorOpen = $state(false);
  let inspectorTab = $state<SlidesTab>("slide");
  let explainOpen = $state(false);
  let findOpen = $state(false);
  let replacing = $state(false);
  let exporting = $state(false);
  let show = $state<{ startAt: number; presenter: boolean } | null>(null);
  let announcement = $state("");
  let windowWidth = $state(window.innerWidth);
  let space = $state({ width: 0, height: 0 });
  /** 1 fits the slide in the window; zooming multiplies it */
  let zoom = $state(1);
  let canvas = $state<ReturnType<typeof SlideCanvas>>();

  const mac = isMac();
  const narrow = $derived(windowWidth < 900);
  // the canvas keeps a little desk around the slide, as keynote does
  const DESK_MARGIN = 32;
  const fit = $derived(fitScale(session.deck, { width: space.width - DESK_MARGIN * 2, height: space.height - DESK_MARGIN * 2 }));
  const scale = $derived(fit * zoom);
  const selected = $derived((session.view?.objects ?? []).filter((object) => session.selection.includes(object.id)));

  function announce(words: string) {
    announcement = "";
    void tick().then(() => (announcement = words));
  }

  // --- what the toolbar and the keys do

  async function newSlide() {
    const layout = newSlideLayout(session.deck.layouts, session.slide?.layoutId ?? null);
    if (!layout) return;
    const index = session.current + 1;
    if (await session.apply({ op: "addSlide", index, layout_id: layout })) {
      await session.go(index);
      announce(`Slide ${index + 1} added.`);
    }
  }

  async function insertTextBox() {
    const at = centredBox(session.deck, 6, 0.4);
    if (await session.insert({ type: "textBox", text: "" }, { ...at, height: 40 })) {
      await tick();
      if (session.selection[0] !== undefined) canvas?.startTyping(session.selection[0]);
    }
  }

  async function insertPictureFile() {
    const path = await choosePictureFile();
    if (!path) return;
    const size = await pictureSize(path).catch(() => ({ width: 4, height: 3 }));
    await session.insertPicture(path, centredBox(session.deck, size.width / Math.max(1, size.height), 0.4));
    announce("Picture added.");
  }

  function onAction(action: ToolbarAction) {
    if (action.startsWith("shape:")) {
      void session.insert({ type: "shape", preset: action.slice(6) }, centredBox(session.deck, 1.6, 0.25));
      return;
    }
    const actions: Record<string, () => unknown> = {
      save: save,
      "save-as": saveAs,
      "export-pdf": () => (exporting = true),
      undo: () => session.undo(),
      redo: () => session.redo(),
      "new-slide": newSlide,
      play: () => play(0, false),
      "play-here": () => play(session.current, false),
      presenter: () => play(session.current, true),
      "text-box": insertTextBox,
      picture: insertPictureFile,
    };
    void actions[action]?.();
  }

  function play(startAt: number, presenter: boolean) {
    (document.activeElement as HTMLElement | null)?.blur();
    show = { startAt, presenter };
  }

  /** text being typed is kept first, so Save never misses the last words */
  async function keepTyping() {
    (document.activeElement as HTMLElement | null)?.blur();
    await tick();
  }

  async function save() {
    await keepTyping();
    if (await session.save()) announce("Saved.");
    return true;
  }

  async function saveAs() {
    await keepTyping();
    return session.saveAs();
  }

  async function restack(to: "front" | "back" | "forward" | "backward") {
    const slide = session.slide;
    if (!slide) return;
    for (const object of selected) await session.apply({ op: "reorder", slide_id: slide.id, object_id: object.id, to }, [...session.selection]);
  }

  function group(apart: boolean) {
    const slide = session.slide;
    if (!slide) return;
    if (apart && selected[0]?.kind === "group") void session.apply({ op: "ungroup", slide_id: slide.id, group_id: selected[0].id });
    else if (!apart && selected.length > 1) void session.apply({ op: "group", slide_id: slide.id, object_ids: selected.map((object) => object.id) });
  }

  function slidesCommand(command: SlidesCommand) {
    const runs: Record<SlidesCommand, () => unknown> = {
      "slides-new-slide": newSlide,
      "slides-duplicate": () => (selected.length > 0 ? session.duplicate() : duplicateSlide()),
      "slides-duplicate-slide": duplicateSlide,
      "slides-delete": () => undefined,
      "slides-group": () => group(false),
      "slides-ungroup": () => group(true),
      "slides-bring-to-front": () => restack("front"),
      "slides-send-to-back": () => restack("back"),
      "slides-bring-forward": () => restack("forward"),
      "slides-send-backward": () => restack("backward"),
      "slides-play-from-start": () => play(0, false),
      "slides-play-from-here": () => play(session.current, false),
      "slides-presenter-view": () => play(session.current, true),
      "slides-replace": () => {
        findOpen = true;
        replacing = true;
      },
      "slides-toggle-notes": () => (notesOpen = !notesOpen),
      "slides-select-all": () => (session.selection = (session.view?.objects ?? []).map((object) => object.id)),
      "slides-export-pdf": () => (exporting = true),
    };
    void runs[command]();
  }

  async function duplicateSlide() {
    const index = session.current;
    if (await session.apply({ op: "duplicateSlide", index })) await session.go(index + 1);
  }

  /** the slide screen's own keys come first: several mean something else on other screens */
  function onKeyCapture(event: KeyboardEvent) {
    if (show || exporting || isTextField(event.target as Element | null)) return;
    const command = slidesCommandForKey(event, mac);
    if (!command) return;
    event.preventDefault();
    event.stopPropagation();
    appCommands.dispatch(command, "keyboard");
  }

  $effect(() => {
    const handlers: [Command, () => void][] = [
      ["save", () => void save()],
      ["save-as", () => void saveAs()],
      ["undo", () => void session.undo()],
      ["redo", () => void session.redo()],
      ["find", () => (findOpen = true)],
      ["zoom-in", () => (zoom = Math.min(4, zoom * 1.25))],
      ["zoom-out", () => (zoom = Math.max(0.25, zoom / 1.25))],
      ["fit-width", () => (zoom = 1)],
      ["toggle-inspector", () => (inspectorOpen = !inspectorOpen)],
      ...SLIDES_COMMANDS.map((name): [Command, () => void] => [name, () => slidesCommand(name)]),
    ];
    const takeBack = handlers.map(([command, handler]) => appCommands.register(command, handler));
    return () => takeBack.forEach((unregister) => unregister());
  });

  // --- leaving, the title, recovery copies

  onMount(() => {
    writerGuard.mayLeave = (ask) =>
      writerMayLeave({ name: session.name, dirty: session.history.dirty }, ask, () => session.save(), () => session.discardRecovery());
    const timer = setInterval(() => void session.autosave(), AUTOSAVE_INTERVAL_MS);
    void session.go(0);
    return () => clearInterval(timer);
  });

  onDestroy(() => {
    writerGuard.mayLeave = null;
    // the shell lets go of the deck and its slide pictures
    void closeDeck(session.id).catch(() => {});
    getCurrentWindow().setTitle("Recents");
  });

  $effect(() => {
    getCurrentWindow().setTitle(session.history.dirty ? `${session.name} — Edited` : session.name);
  });

  async function recover() {
    try {
      const recovered = await openDeck(session.path, true);
      void closeDeck(session.id).catch(() => {});
      session = new DeckSession(recovered, onError);
      recovery = null;
      await session.go(0);
      session.notice = { text: "The recovered version is open. Save to keep it." };
    } catch (problem) {
      session.fail(problem);
    }
  }

  function dismissRecovery() {
    void session.discardRecovery();
    recovery = null;
  }

  // read by the end-to-end scripts (nothing in the app reads it)
  onMount(() => {
    const handle = { session: () => session, scale: () => scale };
    (window as unknown as { __lettersSlides?: typeof handle }).__lettersSlides = handle;
  });
</script>

<svelte:window bind:innerWidth={windowWidth} onkeydowncapture={onKeyCapture} />

<div class="slides-screen">
  <SlidesToolbar
    name={session.name}
    edited={session.history.dirty}
    canUndo={session.history.canUndo}
    canRedo={session.history.canRedo}
    zoom={scale}
    {narrow}
    {railOpen}
    {findOpen}
    {notesOpen}
    {inspectorOpen}
    {explainOpen}
    {onAction}
    onInsertTable={(rows, columns) => void session.insert({ type: "table", rows, columns }, { ...centredBox(session.deck, 2, 0.6), height: rows * 30 })}
    onRail={() => (railOpen = !railOpen)}
    {onRecents}
    onZoomOut={() => (zoom = Math.max(0.25, zoom / 1.25))}
    onZoomIn={() => (zoom = Math.min(4, zoom * 1.25))}
    onFit={() => (zoom = 1)}
    onFind={() => (findOpen = !findOpen)}
    onNotes={() => (notesOpen = !notesOpen)}
    {onNewWindow}
    onInspector={() => (inspectorOpen = !inspectorOpen)}
    onExplain={() => (explainOpen = !explainOpen)}
  />

  {#if recovery}
    <div class="glass notice" role="alertdialog" aria-label="Recovered version">
      <span>Letters kept unsaved changes to this presentation from {new Date(recovery.savedAt * 1000).toLocaleString()}, when it last closed without saving.</span>
      <button class="button" onclick={dismissRecovery}>Discard</button>
      <button class="button primary" onclick={() => void recover()}>Recover</button>
    </div>
  {:else if session.notice}
    <div class="glass notice" role="status">
      <span>{session.notice.text}</span>
      <button class="button" onclick={() => (session.notice = null)}>OK</button>
    </div>
  {/if}

  {#if findOpen}
    <SlidesFind {session} bind:replacing onClose={() => ((findOpen = false), (replacing = false))} onShowNotes={() => (notesOpen = true)} onAnnounce={announce} />
  {/if}

  <div class="body">
    {#if railOpen}
      <SlideRail {session} onNewSlide={() => void newSlide()} onAnnounce={announce} />
    {/if}
    <main class="centre">
      <div class="desk" bind:clientWidth={space.width} bind:clientHeight={space.height}>
        {#if session.slide}
          <SlideCanvas bind:this={canvas} {session} {scale} onAnnounce={announce} />
        {/if}
      </div>
      {#if notesOpen}
        <NotesPane {session} />
      {/if}
    </main>
    {#if inspectorOpen}
      <SlidesInspector {session} tab={inspectorTab} floating={narrow} onTab={(tab) => (inspectorTab = tab)} onClose={() => (inspectorOpen = false)} onAnnounce={announce} />
    {/if}
    {#if explainOpen}
      <SlidesExplain {selected} onClose={() => (explainOpen = false)} />
    {/if}
  </div>

  <div class="visually-hidden" aria-live="polite">{announcement}</div>
</div>

{#if exporting}
  <ExportSheet {session} onClose={() => (exporting = false)} />
{/if}

{#if show}
  <SlideShow
    {session}
    startAt={show.startAt}
    presenter={show.presenter}
    onEnd={(last) => {
      show = null;
      void session.go(last);
    }}
  />
{/if}

<style>
  .slides-screen {
    position: relative;
    display: flex;
    flex-direction: column;
    height: 100%;
    background: var(--desk);
  }

  .body {
    display: flex;
    flex: 1;
    min-height: 0;
  }

  .centre {
    display: flex;
    flex: 1;
    flex-direction: column;
    min-width: 0;
  }

  .desk {
    display: grid;
    flex: 1;
    min-height: 0;
    place-items: center;
    overflow: auto;
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

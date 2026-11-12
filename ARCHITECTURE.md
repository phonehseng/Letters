# Architecture

This explains how Letters is put together and why. It covers what exists
today; planned parts are listed at the end.

## The big picture

```
            window 1            window 2            window 3
        +-------------+     +-------------+     +-------------+
        |  ui (svelte)|     |  ui (svelte)|     |  ui (svelte)|     one web view each
        +------+------+     +------+------+     +------+------+
               |  commands (open, close...)  and  tile images
        +------v-------------------v-------------------v------+
        |                 shell (rust, tauri)                 |     one process
        +--------------------------+--------------------------+
                                   |
        +--------------------------v--------------------------+
        |   pdf worker thread: pdfium + open documents +      |     one thread
        |   newest-first request queue + tile cache (24 mb)   |
        +-----------------------------------------------------+
```

There is **one process** however many windows are open. Starting Letters again,
or double clicking another file, adds a window to the running process. That is
what lets every window share one PDF engine and one cache instead of paying
for its own.

## Stack and why

- **Rust core.** Memory use is predictable and there is no garbage collector
  pausing a scroll. The engine can be tested without any interface.
- **Tauri shell.** It uses the web view the system already has (WebView2 on
  Windows, WKWebView on macOS) instead of shipping a browser, so the program is
  about 5 MB. The cost is that each window's web view uses real memory; see the
  measurements in the changelog.
- **Svelte + TypeScript interface.** Small runtime, readable components, and a
  web surface gives us mature text editing and accessibility for the
  word-processor mode later.
- **PDFium** for reading and drawing PDFs, because it is fast, complete and
  Apache-2.0 licensed. MuPDF was ruled out by its AGPL license.

## Modules

### `core/pdf` (crate `letters-pdf`)

Opens PDFs and renders pages as PNG tiles.

| File | Job |
|---|---|
| `geometry.rs` | the arithmetic of cutting a page into 512 pixel tiles. no PDF types, fully unit tested |
| `cache.rs` | a least-recently-used cache with a hard byte limit |
| `queue.rs` | the waiting line for tile requests: newest first, oldest dropped when too long |
| `engine.rs` | the only file that calls PDFium: open, reload, page sizes, render one tile, read the outline, find text on a page, read where a page's characters are |
| `outline.rs` | the shape of an outline entry and the limits that keep a damaged outline from looping |
| `search.rs` | matching a query against a page's characters, snippets, and turning a match into rectangles |
| `page_text.rs` | grouping a page's characters into lines, giving PDFium's invented spaces a box, and the small cache of recent pages' text. no PDFium types, fully unit tested |
| `worker.rs` | the thread that owns the engine, the queue and the caches, and the handle other code uses to talk to it |
| `error.rs` | every failure, worded for the person using the app |

Decisions worth knowing:

- **Tiles, not whole pages.** A page at high zoom can be tens of megabytes.
  A tile is at most 512 by 512 pixels, and only the tiles on screen are made.
  PDFium draws straight into a tile-sized bitmap, so the rest of the page is
  never allocated.
- **Files are read lazily.** PDFium is given a file reader and loads only the
  parts it needs. A 500 page file is not read into memory.
- **One worker thread.** PDFium is not safe to call from several threads. All
  calls go through one thread, and a process-wide lock guards against a second
  worker ever running at the same time.
- **Newest request first.** When someone scrolls fast, requests pile up. The
  newest is what is on screen now, so it is served first, and requests beyond
  96 waiting are answered with "skipped" (the interface asks again if the tile
  is still visible).
- **Search goes a slice at a time.** A search is sent to the worker 20 pages
  at a time, so tiles are still drawn in between and scrolling does not freeze
  while a long document is searched. Each page's text is dropped before the
  next is read.
- **Scale is an integer.** Tiles are identified by scale in thousandths of a
  pixel per point so the key can be compared exactly.
- **Reload keeps the id.** `RenderWorker::reload(id, path)` points an open
  document at another file under the same `DocumentId`, drops that
  document's cached tiles and returns the new page sizes. Windows keep using
  the id they have. If the new file can't be opened, the old one stays.
  `close` waits until the worker has let go of the file, so the caller can
  delete it straight afterwards.
- **Page text for selecting.** `RenderWorker::page_text(id, page)` returns
  `PageText { chars: [{ ch, rect, line }], lines: [rect] }`: every
  character in reading order with its box in points from the page's
  visible top left (y down, rotation and crop already applied, using the
  same PDFium conversion as tiles and find), and one box per line. Line
  breaks are left out (the line index says where lines change); spaces
  PDFium invents between words are kept and given the gap between their
  neighbours. A page's text is loaded for the call and dropped; the worker
  keeps the last 8 answers and forgets a document's on reload and close.
- **One PDFium call at a time, even when a worker stops.** A worker that
  ends drops its open documents while holding the process-wide lock, so a
  second worker (the tests run several) is never inside PDFium at the same
  moment. Before this the test run crashed now and then.

### `app/shell-core` (crate `letters-shell-core`)

The parts of the shell that need no window, kept apart so they can be unit
tested: the recent files list (`recents.rs`), pinned folders (`pinned.rs`),
reading tile and thumbnail addresses (`address.rs`), finding a file on the
command line (`startup.rs`), the edit session (`session.rs`), the files
page tools make beside the document (`outputs.rs`: reading other PDFs with the
page tools' locked-file message, writing a file the save panel confirmed in
one step, and writing split parts under numbered names that never replace an
existing file), the person's settings (`settings.rs`: the author name on
comments, which starts as the computer's user name) and a picture's size read
from its header (`picture.rs`, so pictures keep their proportions).
`page_tools_tests.rs` and `annotation_tools_tests.rs` run core/pages and
core/annotate through a session the way the app does.

### `app/shell` (crate `letters-shell`, program `letters`)

The desktop program. Thin glue, no logic of its own worth testing.

| File | Job |
|---|---|
| `lib.rs` | starts the app, wires plugins, closes a window's documents when the window closes |
| `state.rs` | what the running app remembers: the worker, the open documents, which window owns which, recents. `AppState::edit` is the door every editing tool uses |
| `documents.rs` | each open document's edit session and page sizes; runs edits, undo, redo and save, and answers with a `DocumentState` |
| `errors.rs` | `CommandError`, the one error every command returns |
| `commands.rs` | the functions the interface can call |
| `page_commands.rs` | one command per page tool from core/pages, each one call through `state.edit` (or `state.read_document` for tools that make other files) |
| `annotate_commands.rs` | one command per annotation operation from core/annotate through `state.edit` (adding answers the new annotation too, so the ui can select it), listing through `state.read_document`, the page text for selecting, a picture's size and the author name |
| `tile_protocol.rs` | serves tiles and thumbnails to `<img>` tags over a private `tile://` address |
| `windows.rs` | creates windows |
| `menu.rs` | the macOS menu bar. each item sends a `menu-command` event to the window in front. Undo and Redo are our own items, so the ui can send them to a text field or to the document |
| `engine_location.rs` | finds the PDFium library (installed app, next to the program, or `vendor/pdfium`) |

Tiles travel as raw image bytes over the `tile://` address, not as JSON
through commands, because encoding megabytes of pixels as JSON is slow.
Responses are marked `no-store` so the web view does not keep a second,
unbounded cache.

### `app/ui`

Logic lives in plain TypeScript modules with unit tests; components stay thin.

| Module | Job |
|---|---|
| `lib/layout.ts` | where pages sit (one column or pairs), which are visible, which tiles they need, zoom and scroll maths |
| `lib/documentState.ts` | the window title ("— Edited"), keeping the reader's place after an edit, the queue that runs changes one at a time, and the unsaved changes question |
| `lib/documentMenu.ts` | what the document menu holds and which of its commands can run now (shortcuts use the same rule) |
| `lib/inspector.ts` | the Tools inspector's tab registry, the `ToolContext` every tool gets, and what Explain lists with a tab open |
| `lib/sidePanels.ts` | which of sidebar, Tools and Explain fit beside the document, and when the inspector floats |
| `lib/selection.ts` | the page selection: click, ctrl or command click, shift click, select all, arrows |
| `lib/movePages.ts` | where moved pages land, the keyboard move, and the drag's drop point and auto-scroll, from layout numbers only |
| `lib/pageRanges.ts` | reading "1-3, 7, 10-" with plain error messages, and writing pages as words |
| `lib/pageEffects.ts` | the "what will happen" sentence under each tool's button |
| `lib/cropRect.ts` | the crop rectangle: starting place, staying on the page (its handles use `pageTools/objectFrame.ts`) |
| `lib/pageTools/pagePoints.ts` | pointer positions to page points and back, at any zoom, in two page mode, at any scroll position |
| `lib/pageTools/textSelection.ts` | selecting text: carets, words, lines, across lines and pages, selection boxes, the selected words, Shift with the arrows |
| `lib/pageTools/gestures.ts` | click counting, click or drag, rectangles and lines from a drag, freehand strokes and grouping them into one drawing |
| `lib/pageTools/objectFrame.ts` | the frame around a selected object: moving, eight handles, keeping proportions, staying on the page, arrow keys |
| `lib/pageTools/pageTool.ts`, `tools.svelte.ts` | the page tool interface, and ready-made tools: rectangle, line, point, freehand ink, text selection |
| `lib/pageTools/pageInput.ts`, `pageTextStore.svelte.ts` | the pointer on the pages handed to the active tool; the text of the pages on screen |
| `lib/annotate/annotations.ts`, `comments.ts`, `palette.ts`, `annotateEffects.ts` | which annotation a click picks, what its frame and style controls allow, comment threads, the colours, and the sentences and fonts of the Annotate tools |
| `lib/annotate/annotateApi.ts` | every annotation call into the shell, typed |
| `lib/annotate/annotateSession.svelte.ts` | what the Annotate tools share while a document is open: the active page tool, the text selection, the annotations, the selected one, a note or text box being written, and each tool's style |
| `lib/fileDrop.ts` | lets the thumbnails claim pdfs dropped from the system |
| `lib/pageApi.ts` | every page tool call into the shell, typed, and the pickers they need |
| `lib/commands.ts` | the one list of commands. keyboard shortcuts and menu events both go through its dispatcher, so they cannot drift apart |
| `lib/search.ts` | moving between matches, the count text, highlight boxes, and running only the newest query |
| `lib/thumbnails.ts`, `lib/outline.ts`, `lib/folders.ts`, `lib/sidebar.ts`, `lib/toolbar.ts` | the rules behind the sidebar, the outline's current section, folder filtering, and what the toolbar drops on a narrow window |
| `lib/api.ts` | every call into the shell, typed |
| `lib/tools.ts` | the label, tooltip, shortcut and explanation of every tool |
| `lib/format.ts`, `lib/keyboard.ts`, `lib/tileRetry.ts`, `lib/dom.ts` | small helpers |

| Component | Job |
|---|---|
| `App.svelte` | one window: Recents or a document; the single keyboard and menu listener |
| `lib/Viewer.svelte` | the viewer's state and wiring |
| `lib/ViewerToolbar.svelte`, `lib/ViewMenu.svelte`, `lib/FindBar.svelte` | the floating toolbar, its View pull-down and the find capsule |
| `lib/DocumentMenu.svelte` | the pull-down on the document's name: Save, Save As, Undo, Redo, Rotate |
| `lib/SaveChangesSheet.svelte` | "Do you want to save the changes…?" before unsaved changes would be lost |
| `lib/PageView.svelte`, `lib/SearchHighlights.svelte` | one page with its tiles, and the match highlights over it |
| `lib/Sidebar.svelte`, `lib/Thumbnails.svelte`, `lib/Outline.svelte` | the left sidebar and its two modes |
| `lib/Home.svelte`, `lib/FolderList.svelte` | the Recents screen and its pinned folders |
| `lib/HelpPanel.svelte` | the Explain side panel |
| `lib/InspectorPanel.svelte` | the Tools inspector: header, tab control, the open tab |
| `lib/inspector/ToolSection.svelte`, `Choices.svelte`, `PagesChoice.svelte` | a tool's disclosure section with its action and effect line; segmented choices; "which pages" |
| `lib/inspector/PagesTab.svelte`, `lib/inspector/pages/*Tool.svelte` | the Pages tab and its eleven tools |
| `lib/inspector/AnnotateTab.svelte`, `lib/inspector/annotate/*Tool.svelte` | the Annotate tab: Selection, Highlight, Underline, Strike Through, Note, Text Box, Shapes, Draw, Picture, Comments, Flatten Comments, Name on Comments |
| `lib/pageTools/PageMarks.svelte`, `lib/pageTools/ObjectFrame.svelte` | a page's live previews and text selection (an svg in page points); the frame with handles around a selected object |
| `lib/annotate/AnnotateOverlay.svelte`, `NotePopover.svelte`, `TextBoxEditor.svelte`, `SelectionBar.svelte`, `Swatches.svelte` | the Annotate controls on a page: the selected annotation's frame, a note's pop-up, a text box typed in place; the bar beside selected text; colour swatches |
| `lib/CropOverlay.svelte`, `lib/ContextMenu.svelte` | the crop rectangle on the page (an ObjectFrame with shading); the thumbnails' right-click menu |
| `lib/ToolButton.svelte`, `lib/Icon.svelte` | toolbar buttons and icons |
| `app.css` | colours, type, shapes: the design tokens (see `docs/DESIGN.md`) |

The viewer keeps only the pages near the screen in the page at all, and for
each of those only the tiles in view. The thumbnail list does the same.
`layout.ts` must agree with `geometry.rs` about page pixel sizes; both have
tests for the same cases.

## How a tile gets on screen

1. The viewer works out which pages overlap the screen (binary search) and
   which tiles of those pages are in view.
2. Each tile is an `<img>` whose address names the document, page, scale,
   column and row.
3. The shell parses the address, refusing anything malformed, and asks the
   worker for the tile on a background thread.
4. The worker returns it from the cache, or renders it with PDFium, encodes it
   as PNG, stores it in the cache and returns it.

## How an edit works

The rule: an edit never touches the person's file until they choose Save.

```
 open      essay.pdf ──copy──> cache/working copies/<run>-<n>/version-0.pdf ──> engine shows it
 edit      version-0 ──lopdf load, closure, save──> version-1 ──reload same id──> engine shows it
 undo      engine reloads version-0 (version-1 is kept for redo)
 save      version-N ──copy──> .essay.pdf.letters-saving ──rename──> essay.pdf
```

1. **Opening** makes a working copy of the file in the app's cache
   (`EditSession::start`) and opens that copy in the engine.
2. **Every version is its own file**, never changed once written. An edit
   loads the current version with lopdf, runs the edit closure on it in
   memory, and saves the result as a new file. The version before becomes
   the undo snapshot as it is, so nothing is copied to make undo possible.
   Because files are never rewritten, the engine (which reads lazily) can
   never see half an edit, and a failed edit leaves the current version
   exactly as it was.
3. **Showing the result**: the worker reloads the document from the new
   file under the same id and drops its tiles. Only if that works does the
   new version become current; otherwise the new file is deleted and
   nothing changed. The revision number goes up.
4. **The ui gets a `DocumentState`** back: `{ id, name, path, pages:
   [{width, height}], revision, dirty, canUndo, canRedo }`. It lays the pages
   out again, keeps the reader's place, and adds `?r=<revision>` to tile
   addresses so the web view fetches new pictures (the shell ignores it).
5. **Undo and redo** keep up to 30 steps each, and at most 512 MB of
   snapshots each, dropping the oldest. The newest step is always kept.
6. **Saving** copies the current version to a temporary file beside the
   original, flushes it to disk and renames it over the original, so a crash
   leaves the old file or the new one, never half of each. A missing or
   read-only original is refused before anything is written, with a message
   that suggests Save As. **Save As** writes the same way to a new place and
   the document then belongs to the new file. Undoing back to the saved
   version clears "Edited" again.
7. **Closing** a document or its window deletes its session folder.
   Folders left by a crash are deleted at the next start (only one copy of
   Letters runs at a time, so any folder not made by this run is a leftover).

Locked (password-protected) PDFs are read only until they are unlocked for
editing in the Protect tab (see "Redaction, hidden information and
passwords (Protect)" below).

### Adding an editing tool

`rotate_pages` in `app/shell/src/page_commands.rs` is the example to copy:

```rust
// in a core crate (here core/pages): a pure function on the lopdf document
// and a settings struct, with an error type whose Display text is shown
pub fn rotate(document: &mut lopdf::Document, params: &RotateParams) -> Result<(), PagesError>;

// in app/shell/src/page_commands.rs: one call through the shared helper
#[tauri::command(async)]
pub fn rotate_pages(state: State<AppState>, id: u64, pages: Vec<u32>, quarter_turns: i32)
    -> Result<DocumentState, CommandError> {
    let params = RotateParams { pages, quarter_turns };
    state.edit(DocumentId(id), |document| page_tools::rotate(document, &params))
}
```

`AppState::edit<E: Display>(&self, id, edit: impl FnOnce(&mut lopdf::Document)
-> Result<(), E>) -> Result<DocumentState, CommandError>` does everything
else: working copy, undo step, reload, revision. In the ui, call the command
through a typed function (`lib/pageApi.ts` for page tools) via the tool
context's `change(action)` (or the viewer's `change(command, action)`), give
the tool an entry in `lib/tools.ts` (the "What happens to the file" line must
say that only the working copy changes until Save), and put it in an
inspector tab (see below), which also lists it in Explain.

### Tools that make other files

Extract, split and merge leave the open document alone. They go through
`AppState::read_document(id, |document| ...)`, which hands over the document as
shown (working copy included) without making an undo step, and write with the
helpers in `outputs.rs`: a file chosen in the save panel is replaced in one
step (the panel already asked); split parts go into the chosen folder under
"<name> - <suffix>.pdf", taking " (2)", " (3)" when a name is taken, and are
created only if new, so nothing there is ever replaced.

### Adding a tab to the Tools inspector

1. Write `app/ui/src/lib/inspector/<Name>Tab.svelte`. It gets one prop,
   `context: ToolContext` (`lib/inspector.ts`): the document, the page being
   read, the target pages (selection or current page), `change(action)` to
   run an edit, `notify`, `showError`, the crop controller, the annotate
   session (page tool, text selection, annotations), and the section a menu
   command asked for.
2. Build it from `ToolSection` components, one per tool: `tool` (a
   `tools.ts` name), `actionLabel`, `effect` (a sentence from a builder like
   those in `pageEffects.ts`), `ready`, `onAction`, the settings as children.
3. Add one entry to `INSPECTOR_TABS`: `{ id, label, tooltip, tools: [...],
   load: () => import("./inspector/<Name>Tab.svelte") }`. Every tool in
   `tools` needs a `tools.ts` entry; Explain lists them first while the tab is
   open, and menu commands can open a tool with `openTool(name)`.

## Working on the page: page tools

Anything done with the pointer on a page (selecting text, highlighting,
drawing a box, placing a note, later drawing a redaction box or clicking a
form field) is a **page tool**. Exactly one is active at a time; the
default is Select (select text, click objects).

```
 pointer on .content ──pageInput.ts──> PagePointer { page, x, y (points), clicks, shift, alt }
        │                                       │
        │                              activeTool.down(pointer) ──> Gesture { move, up, cancel }
        │                                       │
        │                              tool.marks(page) ──> PageMarks.svelte (live preview, svg in points)
        │                                       │
        └── on up, the tool calls its callback ──> an edit through the change queue ──> tile refresh
```

- **Coordinates.** Everything a tool sees is in points from the page's
  visible top left, y down, which is what the engines take. The viewer
  describes each page on screen as a `PageFrame` (`pagePoints.ts`);
  `pageInput.ts` turns a pointer into page points through the frames, at
  any zoom, in two page mode, at any scroll position. A drag stays on the
  page it started on, unless the tool says `crossesPages` (text does).
- **The pointer.** `pageInput` captures the pointer, counts double and
  triple clicks itself, and passes coalesced events while moving, so a
  fast pen stroke keeps its points. Controls floating over pages (a frame,
  a pop-up, a text box being typed, the selection bar) carry
  `data-page-ui` and keep their own presses. Escape during a drag cancels
  it.
- **Previews, then the real thing.** While a gesture is under way the tool
  returns `marks` (rectangles, ellipses, lines, ink) that `PageMarks.svelte`
  draws in an svg whose coordinates are page points. When the gesture ends
  the tool's callback makes one edit; the engine draws the result and it
  arrives with the normal tile refresh.
- **Text.** `PageTextStore` fetches `page_text` for the pages on screen
  once scrolling rests and keeps it for the current revision. The text
  selection (`textSelection.ts`) is shared by every tool, so "select, then
  pick Highlight" and "pick Highlight, then drag" both work.
- **Selected objects.** `ObjectFrame.svelte` is the frame with eight
  handles (four for pictures, which keep their proportions; none when the
  object can't be resized). Arrows move by 1 point, Shift by 10, Alt with
  the arrows resizes, Delete deletes, Escape lets go. Arrow presses are
  committed when they rest for a moment, so holding a key makes one undo
  step. The crop rectangle is the same frame with shading.

### The page tool interface (`lib/pageTools/pageTool.ts`)

```ts
interface PageTool {
  id: string;                 // "select", "highlight", "shape-square", "redact-area", ...
  cursor: string;             // the css cursor over pages
  crossesPages: boolean;      // a drag follows the pointer to other pages
  down(pointer: PagePointer): Gesture | null;   // null ignores the press
  marks(page: number): Mark[];                  // live previews on that page
  key?(event: KeyboardEvent): boolean;          // keys while the document has the focus
  finish?(): void;                              // another tool takes over: save or drop what waits
}
```

### Adding a page tool

Most tools are one call to a factory in `lib/pageTools/tools.svelte.ts`:

| Factory | Gesture | Callback |
|---|---|---|
| `rectangleTool({ id, style, pageSize, onDraw, ... })` | drag a box (Shift for a square; `aspect` keeps proportions; `clickSize` places a box on a click) | `onDraw(page, rect)` |
| `lineTool({ id, style, arrow, pageSize, onDraw })` | drag a line (Shift for 45 degree steps) | `onDraw(page, start, end)` |
| `pointTool({ id, onPlace })` | click a place | `onPlace(page, point)` |
| `inkTool({ id, style, onDrawing })` | freehand strokes, grouped by pauses | `onDrawing(page, strokes)` |
| `textTool({ id, host, pick?, onSelected? })` | select text by drag, double or triple click, Shift click, Shift with the arrows | `onSelected(selection)`; `pick(pointer)` takes presses on objects first |

The exact steps, using redaction's "draw boxes or select text to redact" as
the example:

1. **The edit.** Add a command in `app/shell/src/` that calls the engine
   through `state.edit` (copy `add_shape` in `annotate_commands.rs`), and a
   typed function for it in the ui (copy `addShape` in
   `lib/annotate/annotateApi.ts`).
2. **The tool.** Make it with a factory, giving the edit as the callback:
   ```ts
   const redactArea = rectangleTool({
     id: "redact-area",
     style: () => ({ stroke: "black", fill: "rgb(0 0 0 / 0.3)", width: 1, opacity: 1 }),
     pageSize: (page) => doc.pages[page],
     onDraw: (page, rect) => change(null, () => markForRedaction(doc.id, page, [rect])),
   });
   const redactText = textTool({
     id: "redact-text",
     host: textHost, // the same TextHost the annotate session builds (text, selection, setSelection, currentPage, pageCount)
     onSelected: (selection) => {
       for (const page of selectedPages(selection)) {
         const rects = selectionRects(selection, page, texts.get(page)!);
         // one edit for all pages, as applyMark in annotateSession.svelte.ts does
       }
     },
   });
   ```
   Keep the tools in a small session class like `AnnotateSession` (one per
   viewer) with a `tool` field and a `pageTool` derived from it, so the
   viewer always has one active tool.
3. **Make it the active tool.** The viewer hands `pageInput` the active
   tool (`tool: () => activeSession.pageTool`) and draws
   `pageTool.marks(page)` for each page. If two sessions exist (annotate and
   redact), give the viewer one `activePageTool` that is whichever session
   last picked a tool, and have each session's `setTool` call `finish()` on
   the other's tool.
4. **A section in a tab.** A `ToolSection` whose button picks the tool
   (`pressed` while it is active), with a `tools.ts` entry. A press of the
   button from the keyboard has `event.detail === 0`: place something in the
   middle of the view (`viewCentre()` in the viewer) instead, so the tool
   works without a pointer.
5. **Escape and focus.** Put the tool's "drop what is half done" in the
   session's `escape()`; the viewer calls it.

Forms ("click a field to fill") are `pointTool` or a `textTool` `pick` that
asks "is there a field here?" and opens an editor on the page. Editors on
pages are positioned from the page's `PageFrame` like
`annotate/TextBoxEditor.svelte`, and carry `data-page-ui`.

**Reusing text selection** without a tool of your own: read the shared
selection (`annotate.selection`) and turn it into per-page boxes with
`selectionRects(selection, page, annotate.texts.get(page))`, or its words
with `selectedText`. **Reusing rectangle drawing**: `rectangleTool` above,
or the pure `rectFromDrag(start, end, pageSize, square)` in `gestures.ts`.

## How an annotation works

1. The Annotate tab (or the selection bar, or a shortcut) picks a page tool
   or acts on the selection.
2. The tool's gesture ends and `AnnotateSession` calls the shell
   (`add_text_markups`, `add_note`, `add_shape`, ...) through the viewer's
   change queue: one `state.edit`, one undo step, "Edited" in the title.
   New annotations are written by core/annotate with their own appearance
   streams, so every PDF reader shows them the same way.
3. The shell answers with the new `DocumentState` and the annotation as the
   engine wrote it; the session selects it at once, and a live region says
   "Highlight added to page 3."
4. After every new revision the session reads `list_all_annotations` once
   (one load of the working copy) and the page text again, so undo, redo
   and edits from other tools are reflected in the frames, the comments
   list and hit testing.
5. Moving, resizing and restyling are `update_annotation`; deleting is
   `delete_annotation` (replies go with their note); flattening is
   `flatten_annotations`, which refuses when there is nothing to flatten
   instead of making an empty undo step.

## Word-processor mode (the writer)

`.docx` and `.odt` files open in the writer instead of the viewer. The
file layer is `core/office` (one model for both formats, unknown content
kept); the writer adds an editing surface on top of it.

```
 file ──writer_open──> shell keeps carry + picture bytes (shell-core/src/writer)
                    └> ui gets the model json ──convert.ts──> prosemirror document
 typing, styles, lists, tables, tracked changes: prosemirror transactions (undo = its history)
 save / autosave / export: ui sends docToModel(doc) ──> shell merges carry + pictures ──> letters-office writes
```

- **Routing.** `App.svelte` asks `writer/routing.ts` (the shell has the
  same rule in `shell-core/src/writer/routing.rs`): `.pdf` to the viewer,
  `.docx`/`.odt` to `writer/WriterScreen.svelte`, anything else to a plain
  "can't open yet" message. The window's Open… panel offers both kinds.
- **Shell** (`app/shell/src/writer_commands.rs`, state managed beside
  `AppState`): `writer_open`, `writer_save` (Save and Save As; the format
  follows the name), `writer_odt_losses` (what ODT can't keep, asked before
  saving as .odt), `writer_autosave` / `writer_discard_recovery` (recovery
  copies), `writer_media` and `writer_add_picture` (pictures as `data:`
  addresses), `writer_apply_preset` and `writer_describe_ruler` (the
  engine's MLA/APA and ruler sentences), `writer_export_pdf`,
  `writer_settings`. All the logic is in `shell-core/src/writer/*`, unit
  tested: `sessions.rs` (open documents), `document.rs` (what the ui never
  sees), `save.rs` (safe save: temp file, flush, rename), `recovery.rs`,
  `export.rs` (+ `word_export.ps1`), `settings.rs`, `engine.rs`.
- **The model in the editor** (`writer/schema.ts`, `writer/convert.ts`).
  Every node and mark keeps its model object (minus children) in
  `attrs.data`; the document node keeps everything but the body (styles,
  numbering, sections, headers and footers, notes, comments, settings) in
  `attrs.meta`, changed with `setDocAttribute` so Undo covers it. Marks are
  declared in the engine's `Mark::rank` order, so their order survives.
  Model -> editor -> model is identical when nothing was edited
  (`convert.test.ts`, including a document Word made and its ODT copy).
- **Editing logic** is in pure modules with tests: `editing.ts` (styles,
  marks, paragraphs, lists, indents, inserting, the page), `typing.ts` (a
  port of the engine's `on_type`, same test cases: it runs on every key, a
  shell round trip would be felt), `tracked.ts` (listing, accepting,
  rejecting, and recording edits while Track Changes is on), `outline.ts`,
  `find.ts`, `counts.ts`, `pagination.ts`, `ruler.ts`, `indentation.ts`,
  `numbering.ts`, `styles.ts`, `css.ts`, `shortcuts.ts`.
- **Behaviour** is in `writer/plugins.ts`: keys (Enter follows the style's
  "next", Tab and Shift Tab), typing rules, list labels and note numbers
  (decorations), find highlights, page gaps, the guard that keeps preserved
  items from being deleted by accident, and unique Word paragraph ids.
- **The session** (`writer/writerSession.svelte.ts`) owns the editor view,
  mirrors its state into a Svelte field for the screen, and runs every
  command by name, so menus, keys, toolbar and inspector do the same thing.
- **Pages** (`writer/PageSurface.svelte`, `pageGaps.ts`, `pagination.ts`).
  The page size and margins come from the first section. After each
  change the surface measures every top-level block and places a page gap
  (rest of page, footer, desk, next header) where the next block no longer
  fits. This is an estimate: exact page breaks come from Word or
  LibreOffice on export. Footnotes are shown together under the text.
- **Commands**: the writer's names are in `writer/writerCommands.ts` and
  join `COMMANDS` in `lib/commands.ts`; its keys are in `writer/shortcuts.ts`
  and are matched before the app's (capture phase), because Ctrl L, Ctrl R
  and Ctrl Alt 1-3 mean other things in the viewer. The macOS Format and
  Insert menus are `app/shell/src/writer_menu.rs`.
- **Leaving.** The window's unsaved-changes sheet asks the writer through
  `writer/writerGuard.ts`; "Don't Save" also deletes the recovery copy.
- **Where page breaks come from** (`writer/layoutSource.ts`): one interface,
  three providers in order: Word's real layout (Strict Word mode, Word
  installed: `shell-core/src/writer/layout.rs` + `word_layout.ps1` give the
  page each block starts on), Letters' own engine (`core/layout`, when it
  lands: add it to `chooseProvider`), and the on-screen estimate. The
  status bar names the one in use. Strict mode's feature list and
  compatibility line are behind `writer/strictMode.ts`, to be fed by the
  engine's `feature_matrix()` / `word_compatibility()`.
- **Pictures** (`writer/ImageFrame.svelte`, `writer/resize.ts`): resizing
  previews with a css transform and commits once (one undo step); Free
  pictures are drawn from their paragraph's top left by the image node view
  (`images.ts`); paste and drop go through `writer_add_picture_data`.
- **Saving elsewhere**: `writer/formats.ts` (the Save As list and its
  notes; `writer_export_copy` writes html and text through the engine's
  exporters), `writer/print.ts` (print views from pdf engine tiles at 300
  dpi; writer documents print Word's print-ready pdf), and
  `shell-core/src/drive/` (Google Drive: the desktop folder, or direct
  upload with PKCE, a loopback redirect, `drive.file` scope and the system
  credential store; tested against `fake_google.rs`, a local stand-in;
  commands in `app/shell/src/drive_commands.rs`).
- **Fonts** (`writer/fonts.ts`) is the adapter for the font library
  (`core/fonts`, `lib/fonts`): metric-compatible stand-ins in every font
  stack, a missing-font count, and a FontFace loader for served bytes.

## Forms and signatures (Fill & Sign)

The engine is `core/forms` (see its README). The app's parts:

| Where | Job |
|---|---|
| `app/shell/src/forms_commands.rs` | one command per operation: `forms_overview` (fields, the engine's xfa message, whether the form calculates, the signatures checked, whether the saved file is guarded), `forms_set_value`, `forms_add_fields` (several in one undo step), `forms_update_field`, `forms_remove_field`, `forms_detect_fields`, `forms_flatten`, `forms_place_signature`, the kept signature pictures, `forms_create_certificate`, `forms_sign`, `forms_release_signature_guard` |
| `app/shell/src/forms_menu.rs` | the macOS Forms menu |
| `app/shell-core/src/forms/` | `signed.rs` (signing through the edit session), `scripts.rs` (does the form calculate), `signature_pictures.rs` (kept signatures in the app data folder), `files.rs` (reading chosen files, writing the certificate, data addresses), `tests.rs` |
| `app/ui/src/lib/forms/` | `fillSignSession.svelte.ts` (what the tools share), the pure modules `tabOrder.ts`, `fieldEditing.ts`, `validation.ts`, `proposals.ts`, `newFields.ts`, `signaturePicture.ts`, `signatures.ts`, `fillSignEffects.ts` (all tested in `fillSign.test.ts`), the page parts `FormsOverlay.svelte`, `FieldBox.svelte`, `TextFieldEditor.svelte`, `ChoiceEditor.svelte`, the window parts `FormsChrome.svelte` and `SignedEditSheet.svelte`, the signature makers `SignaturePad.svelte`, `TypedSignature.svelte`, `ImageSignature.svelte`, and `fillSignTools.ts`, `fillSignCommands.ts`, `formsApi.ts` |
| `app/ui/src/lib/inspector/FillSignTab.svelte`, `inspector/fillsign/*` | the tab and its sections |

- **Filling is not a page tool.** Each field's place is a tinted box in
  `FormsOverlay` (with `data-page-ui`, so the active page tool never sees
  the press). A text, date or choice field opens its editor right there;
  checkboxes and radio buttons change on a click or Space. The editor
  keeps the typing and sends it once, when the field is left, so an
  answer is one undo step. A refusal from the engine is caught inside
  the change (which then answers the document as it was) and shown
  beside the field instead of as a window message.
- **Tab order** (`tabOrder.ts`): one stop per field, page by page, in rows
  (top to bottom, left to right). Tab is handled by the editors and boxes
  themselves, so it crosses pages: the next field is scrolled into view
  and its editor takes the focus when it appears.
- **Page tools** of Fill & Sign (drawing a field, placing a signature
  picture, marking where a digital signature goes) are `rectangleTool`s
  in the session. The viewer's one active tool is whichever session picked
  a tool last (`protect.pageTool ?? fillSign.pageTool ?? annotate.pageTool`).
- **Signing keeps exact bytes.** Every other edit loads the working copy
  with lopdf and writes it whole, which would break a signature. Signing
  goes through `OpenDocuments::with_session` and
  `forms/signed.rs::sign_saved_document`: the document must be saved (so
  the working copy is the file on disk), core/forms' `sign` appends an
  incremental update to those bytes, and `EditSession::adopt_exact_bytes`
  makes them the new version and writes them over the file without lopdf.
  The session is then protected: `save` refuses to write a changed version
  over a signed file (`SessionError::SignedOriginal`, which points to Save
  As), and Save As of a changed document clears the guard for the copy.
  A signed file opened unchanged gets the same guard when its signatures
  are checked.
- **The question before changing a signed document.** The viewer's
  `change` asks `fillSign.mayChange` first (undo and redo never ask).
  Save a Copy saves to a new file, releases the guard and goes on; Edit
  Anyway goes on (its Save is then refused); Cancel makes no change.
- **Calculations** are never run. `scripts.rs` looks for a calculation
  order or calculate actions so the tab can say so plainly.

## Redaction, hidden information and passwords (Protect)

The engine is `core/redact` (see its README, especially "Known limits").
The app adds the rules that keep a removed secret from surviving on disk.

| Where | Job |
|---|---|
| `app/shell/src/protect_commands.rs` | `find_redaction_text`, `redaction_preview`, `redaction_dry_run` and `close_redaction_dry_run` (the review), `apply_redactions`, `hidden_information`, `remove_hidden_information`, `protection_status`, `unlock_document`, `change_lock_on_save`; and for the rest of the shell `note_opened_with_password`, `hides_thumbnail`, `forget_document` |
| `app/shell/src/protect_menu.rs` | the macOS Protect menu |
| `app/shell-core/src/protect/` | `clean_save.rs` (the save writer and `LockOnSave`), `unlock.rs` (how a file is locked, unlocking for editing, checking an owner password), `permissions.rs` (a file's /P bits as the engine's options), `protection.rs` (each document's protection, `merge_lock`, the status the ui shows), `tests.rs` |
| `app/ui/src/lib/protect/` | `protectSession.svelte.ts` (marks, the two page tools, review, apply, passwords, the close question), the pure modules `pendingMarks.ts`, `review.ts`, `findMarks.ts`, `passwordStrength.ts`, `permissions.ts`, `scrubItems.ts`, `protectEffects.ts`, `protectGuard.ts` (tested in `protect.test.ts`), the page part `RedactionMarks.svelte`, the sheets `ProtectSheets.svelte`, `ReviewSheet.svelte`, `ApplySheet.svelte`, `DiscardMarksSheet.svelte`, `ProtectSheet.svelte`, and `protectTools.ts`, `protectCommands.ts`, `protectApi.ts` |
| `app/ui/src/lib/inspector/ProtectTab.svelte`, `inspector/protect/*` | the tab and its sections |

- **Marks are not edits.** Marking (text selection, a box, Find and Mark)
  only adds to the session's `marks`; nothing reaches the shell until
  Apply. `RedactionMarks` draws them as red, dashed, hatched boxes, and a
  picked one gets the shared `ObjectFrame`. A mark remembers what its page
  looked like (size and the start of its text); after any change by
  another tool, marks on a page that looks different are dropped with a
  notice instead of landing on other words. Closing the document asks
  first (`protectGuard.ts`, called by `App.svelte`).
- **The review never touches the working copy.** `redaction_preview` gives
  the exact text, pictures, drawings, comments and form values per box.
  The engine's preview has no warnings, so `redaction_dry_run` redacts a
  clone of the document, writes it into the session's folder and opens it
  in the worker under its own id: the sheet draws its tiles as the "after"
  picture and shows that run's warnings. It is closed when the sheet
  closes, and with the document.
- **Apply** is one `state.edit`: `redact`, then `rewrite_clean` (the
  engine's `save_clean` and a reload in memory), so the version stored in
  the cache keeps no unused object with the old content either.
- **Saving after a redaction or a scrub can't be a plain copy.** The
  command then calls `EditSession::write_saves_with(clean_writer(lock),
  true)`. From then on `save` and `save_as` write what the writer makes
  (the current version loaded, locked if asked, written by `save_clean`)
  instead of copying the version file. There is no way to switch it off,
  and every later save of the session is clean. The `true` makes a
  successful save delete every undo and redo version
  (`forget_history`), because the versions before the redaction still
  hold the secret. Closing deletes the session folder; a crash leaves it
  until the next start, when `clean_leftover_sessions` deletes it.
- **A password is applied when the file is saved**, never to the working
  copy (the viewer has to keep showing it). `change_lock_on_save` merges a
  change into the document's `LockOnSave` (a new open password keeps the
  restrictions, and the other way round), gives the session a clean
  writer that encrypts with AES-256, and marks the document unsaved so
  Save is offered. No picture of a document that needs a password to open
  is kept for Recents (`hides_thumbnail`, called by `commands.rs`).
- **Unlocking for editing.** A locked working copy is still view only
  (`SessionError::PasswordProtected`). `unlock_document` checks the
  password (open or owner; for a file with only restrictions that forbid
  editing, only the owner password, checked by rebuilding its /Encrypt
  from lopdf's state), decrypts with the engine's `unlock`, and
  `EditSession::start_over_unlocked` makes the decrypted bytes the only
  version (the locked one is deleted). Save then locks it again the same
  way: AES-256 with the open password and the original permissions; a
  file unlocked with its owner password has an unknown open password, so
  save is refused (`LockOnSave::Undecided`) until the person chooses.

## Planned

In the order they will be built (each plugs into the edit pipeline above and
gets a tab in the Tools inspector): forms and signing,
OCR, redaction, compare, word-processor mode, conversion with the fidelity
harness, optional AI tools, the accessibility wizard, packaging and default-app
registration. Each will get its own crate under `core/`.

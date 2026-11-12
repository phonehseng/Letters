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
| `command_routes.rs` | every command the ui can call, one small block per feature; each block's `feature!` list makes both its handler and its names. a new command is one line in its feature's block |
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
  `.docx`/`.odt`/`.rtf`/`.doc` to `writer/WriterScreen.svelte`, anything
  else to a plain "can't open yet" message. Rich text and old Word files
  open through the engine's `formats::open_any` (a `.doc` through Word or
  LibreOffice) and save as a `.docx` beside the original.
- **Shell** (`app/shell/src/writer_commands.rs`, state managed beside
  `AppState`): `writer_open`, `writer_save` (Save and Save As; the format
  follows the name), `writer_odt_losses` (what ODT can't keep, asked before
  saving as .odt), `writer_autosave` / `writer_discard_recovery` (recovery
  copies), `writer_media` and `writer_add_picture` (pictures as `data:`
  addresses), `writer_apply_preset` and `writer_describe_ruler` (the
  engine's MLA/APA and ruler sentences), `writer_export_pdf`,
  `writer_settings`, and since the second round `writer_objects` (text
  blocks, free pictures, stacking, aligning, Word fixes), `writer_compatibility`,
  `writer_copy`, `writer_paste_picture`, `writer_paste_html`,
  `writer_formats`, `writer_system_print` and (in `drive_commands.rs`)
  `writer_word_layout`, `writer_engine_layout`, `writer_print_pdf`. All the
  logic is in `shell-core/src/writer/*`, unit tested: `sessions.rs` (open
  documents), `document.rs` (what the ui never sees), `save.rs` (safe save:
  temp file, flush, rename), `recovery.rs`, `export.rs` and `layout.rs`
  (Word through the engine's `bridge`: a private hidden Word found by its
  window caption, with a watchdog; LibreOffice when Word is missing),
  `engine_layout.rs` (`core/layout`), `objects.rs` (the engine's `floating`
  and `compat`), `clipboard.rs` (the engine's `clipboard` and html import),
  `settings.rs`, `engine.rs`.
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
- **Pages** (`writer/PageSurface.svelte`, `pageGaps.ts`, `pagination.ts`,
  `surfaceLayout.ts`). The page size and margins come from the first
  section. After each change the surface measures every top-level block,
  and the lines of paragraphs tall enough to split, and places a page gap
  (rest of page, footer, desk, next header) where the next block no longer
  fits, or inside a paragraph before the line that no longer fits, with
  Word's rules: two lines at least on each side (widows and orphans), keep
  with next, keep lines together. Footnotes are drawn at the bottom of the
  page their reference is on, and their height counts against that page.
- **Objects on the page** (`textBlocks.ts`, `objectGeometry.ts`,
  `objectLayer.ts`, `wrap.ts`, `snapping.ts`, `crop.ts`, `frameMaths.ts`,
  `writerObjects.svelte.ts`, `ObjectFrame.svelte`, `ObjectBar.svelte`).
  A text block is the model's `textBox` block, a `text_box` node holding
  ordinary blocks. Free pictures and text blocks are absolutely placed in
  the editor root from their model position (page, margin, column or
  paragraph, in twips) by node decorations; text wraps around them through
  an invisible css float with `shape-outside` in the paragraph above each
  object (wrap.ts explains why floats). Commands that change structure
  (split, make, return to text, in line or free, front, back, align,
  distribute, Word fixes) run in the engine and come back as one
  transaction that replaces only the changed blocks (`applyEngineModel`),
  so each is one undo step; moving, resizing, cropping and turning are
  local, previewed on the object's own element while dragging, written
  once on release. Snapping is a port of the engine's `floating::snap`
  with its test case, because it runs on every pointer move.
- **Strict Word mode** (`strictMode.ts`, `CompatibilitySheet.svelte`,
  `wordLines.ts`). What is offered comes from the engine's
  `compat::feature_matrix()`; the status line, its Show list and Fix come
  from `word_compatibility()` and `apply_fixes()`. Word's layout
  (`bridge::word_layout`, cached by a fingerprint of the file) gives every
  line's paragraph and first character; the editor forces its lines to
  break there (`white-space: pre` and a break widget per line), puts page
  gaps where Word's pages start, and drops Word's breaks in a paragraph
  that is edited until the next check (4 s after typing pauses, or Check
  Now). A paragraph whose text doesn't match Word's line keeps its own
  wrapping.
- **Commands**: the writer's names are in `writer/writerCommands.ts` and
  join `COMMANDS` in `lib/commands.ts`; its keys are in `writer/shortcuts.ts`
  and are matched before the app's (capture phase), because Ctrl L, Ctrl R
  and Ctrl Alt 1-3 mean other things in the viewer. The macOS Format and
  Insert menus are `app/shell/src/writer_menu.rs`.
- **Leaving.** The window's unsaved-changes sheet asks the writer through
  `writer/writerGuard.ts`; "Don't Save" also deletes the recovery copy.
- **Where page breaks come from** (`writer/layoutSource.ts`): one interface,
  three providers in order: Word's real layout (Strict Word mode, Word
  installed: `shell-core/src/writer/layout.rs`), Letters' own engine
  (`core/layout` through `engine_layout.rs`, a setting in the Document tab
  for Free mode; same answer shape as Word's, applied the same way), and
  the on-screen estimate. The status bar names the one in use.
- **Pictures** (`images.ts`, `crop.ts`, `resize.ts`): a cropped picture is
  a window onto the whole picture at its scale; pasted pictures, picture
  files and html with pictures go through the engine
  (`clipboard::add_image`, `import::from_html_with_images`); copying a
  picture puts png bytes on the system clipboard (`clipboard::copy_blocks`).
- **Saving elsewhere**: `writer/formats.ts` (the Save As list from the
  engine's `format_capabilities()`; `writer_export_copy` writes any of
  `formats::save_as`'s formats, Markdown with its pictures beside it),
  `writer/print.ts` (print views from pdf engine tiles at 300 dpi, then
  Windows' own print dialog through the web view's `ShowPrintUI`; writer
  documents print the bridge's print-ready pdf), and
  `shell-core/src/drive/` (Google Drive: the desktop folder, or direct
  upload with PKCE, a loopback redirect, `drive.file` scope and the system
  credential store; tested against `fake_google.rs`, a local stand-in;
  commands in `app/shell/src/drive_commands.rs`).
- **Fonts** (`writer/fonts.ts`) uses the font library (`lib/fonts`): the
  Format inspector's font control is its `FontPicker`, fonts the document
  uses that aren't installed are loaded from the bytes the shell serves
  (`useFontInDocument`), the status bar's missing-font count opens its
  Missing Fonts view, and Replace there changes the font everywhere in
  one undo step. Metric-compatible stand-ins stay in every font stack.
- **Tracked changes** (`tracked.ts`, `trackedFormatting.ts`): typing and
  deleting are change marks; formatting changes are Word's `w:rPrChange`
  kept as a preserved run property; splitting and joining paragraphs are
  revisions of the paragraph mark (`w:ins`/`w:del` in its `w:rPr`), so all
  of them save as real Word revisions.

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
  are checked. With Protect: when saves are rewritten (a clean save after
  a redaction) the file on disk is signed, not the working copy, and
  signing then deletes the undo versions as a clean save does; Save As
  of the unchanged signed file copies it exactly. A password or
  restrictions (Protect's `ProtectionTable`, or a locked file on disk)
  make signing refuse with a plain sentence, since the engine signs only
  unencrypted files and locking afterwards would break the signature.
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

## The font library

The engine is `core/fonts` (crate `letters-fonts`; its README lists the
sources, the licence of everything offered, and what leaves the computer).

```
 system folders ──scan (own thread, cache by path+size+mtime)──┐
 <app data>/fonts/added      (fonts the person adds)           ├─> FontIndex: families ──> search / similar / resolve
 <resources>/fonts           (bundled, vendor/fonts in dev)    │                         ──> previews, embedding data
 <app data>/fonts/library    (downloaded)                      │
 data/catalogue.z            (2,042 online families, offline) ─┘
```

| Where | Job |
|---|---|
| `core/fonts/src/library.rs` | `FontLibrary`: the one object the app holds. scans in the background, rebuilds the index as batches arrive (a revision number goes up), adds and removes fonts, downloads on a thread per family, keeps favourites, recents and the one-time download notice in `<app data>/fonts/settings.json` |
| `core/fonts/src/{container,sfnt,woff,woff2}.rs` | reading font files safely: size limits, the format from the first bytes, WOFF and WOFF2 decoded by bounds-checked code |
| `core/fonts/src/{facts,coverage}.rs` | the facts of a face, writing systems from the character map, the fsType sentence |
| `core/fonts/src/{search,fuzzy,similar,substitute}.rs` | pure functions over the index: forgiving search, look-alikes (same widths first), substitutes and fallbacks |
| `core/fonts/src/{outline,raster,preview,subset}.rs` | previews as svg paths or alpha bitmaps by our own code; subsets for embedding |
| `core/fonts/src/{catalogue,download}.rs` | the shipped catalogue of the online library (made by `examples/build_catalogue.rs`), and downloads checked against its git blob hashes |
| `app/shell-core/src/fonts/` | the library's folders and finding the bundled fonts |
| `app/shell/src/fonts_commands.rs` | one command per question (`fonts_status`, `fonts_families`, `fonts_search`, `fonts_details`, `fonts_face_bytes` as raw bytes, `fonts_preview`, `fonts_add`, `fonts_remove`, `fonts_download`, `fonts_missing`, ...) |
| `app/ui/src/lib/fonts/` | `FontLibrary.svelte` (the library sheet), `FontDetail.svelte`, `MissingFonts.svelte`, `FontPicker.svelte` (the reusable picker), `FontSample.svelte`; the pure modules `virtualList.ts`, `sampleText.ts`, `fontQuery.ts`, `pickerRows.ts`, `missingFonts.ts` (tested in `fonts.test.ts`); `fontStore.svelte.ts` (what the library and pickers share), `fontFaces.ts` (FontFace registration), `fontsApi.ts`, `fontTools.ts`, `fontCommands.ts` |

- **Search runs in the engine** on every keystroke; the ui loads the family
  list once per revision and gets back positions in it, so an answer is a
  few kilobytes. Only the newest answer is shown (`latestOnly`).
- **Lists are virtual** (`virtualList.ts`): only rows on screen exist, so
  2,274 families scroll as quickly as twenty.
- **Fonts that aren't installed** (added, bundled, downloaded) reach the web
  view as raw bytes through `fonts_face_bytes` and are registered with the
  FontFace api for that window only; nothing is installed on the system.
  System fonts are drawn by name.
- **Opening the library**: `openFontLibrary(options)` mounts it over any
  screen; the `font-library` command (Ctrl T, Command T on a mac) is
  registered once in `lib/commands.ts`. A screen with a document calls
  `setDocumentFonts({ documentFonts, onReplaceFont })` so the library's
  Missing Fonts view knows the document's fonts.
- **The picker** for the writer and the PDF text tools: `import { FontPicker }
  from "lib/fonts"`, props `value`, `onChange`, `documentFonts`, `label`,
  `disabled` (documented in `lib/fonts/index.ts`).
- **Embedding**: `letters_fonts::embedding_data` checks fsType and returns a
  TrueType subset (glyph numbers kept) or the whole font. core/edit still
  has its own copy of the subsetter; it and the docx writer should move to
  this one.

## Comparing two versions (Compare)

The engine is `core/compare` (see its README). Compare is a screen of its
own, not an inspector tab: two documents are shown, and neither changes.

| Where | Job |
|---|---|
| `app/shell/src/compare_commands.rs` | `compare_start` (two inputs: a file, or the open document, which is copied as it is now), `compare_status` (polled), `compare_result`, `compare_cancel`, `compare_close`, `compare_pair_pictures` (old, new and overlay of one pair as `data:` pngs), `compare_save_report` (pdf, html or text); `CompareState` beside `AppState`; a window's comparisons close with it |
| `app/shell-core/src/compare/` | `job.rs` (one comparison on its own thread: both files read with lopdf for the engine and opened in the pdf worker under their own ids for drawing; progress, cancel, pictures, report, `close` closes both and deletes the job folder), `jobs.rs` (every job by window; the `comparisons` cache folder, emptied at start), `renderer.rs` (whole pages stitched from worker tiles, png in and out), `tests.rs` (against pdfium) |
| `app/ui/src/lib/compare/` | `CompareScreen.svelte` and its parts (`CompareSetupView`, `CompareProgress`, `CompareToolbar`, `CompareColumn`, `ChangeMarks`, `OverlayView`, `ChangesList`, two sheets); the pure modules `changes.ts` (grouping, filters, next and previous, wording, mark tones), `scrollSync.ts` (slots per pair, the engine's scroll map ported from `map_scroll`, following scroll), `changeList.ts` (the virtual list), `overlay.ts` (slider layers, Flash), `progress.ts`, `setup.ts`, all tested in `compare.test.ts`; `compareKeys.ts` (the screen's own dispatcher) |

- **A screen over the document.** `App.svelte` keeps the viewer (or
  Recents) mounted underneath, `visibility: hidden` and `inert`, so its
  scroll, tools and unsaved marks survive; while Compare shows, keys and
  menu commands go to `compareCommands` instead of `appCommands`, so the
  document's own handlers are never overwritten. Back sets `comparing`
  to null and everything underneath is as it was.
- **Slots line the columns up.** Both columns have one slot per page pair;
  a page only one side has gets a dashed placeholder of the other side's
  size. Scrolling one column finds the pair and height under the toolbar
  (`anchorAt`), maps the height through the pair's scroll map, and sets
  the other column; the programmed scroll is recognised and not echoed.
- **Only what is near the screen exists**: page slots (`visiblePages`),
  tiles in view (`tilesForPage`), list rows (`changeList.ts`), and at most
  8 overlay pictures, asked for at the screen's scale.
- **Locked files** are refused by `LopdfSource` with the engine's own
  message before anything is opened in the worker.

## The accessibility checker and fix wizard (Accessibility)

The engine is `core/a11y` (see its README). Checking reads; every fix is
one `state.edit`, so it is one undo step and the file changes only on Save.

| Where | Job |
|---|---|
| `app/shell/src/a11y_commands.rs` | `a11y_check` (the report plus `machineChecksPass`, the engine's own rule for offering Mark as PDF/UA), `a11y_fix` (one `FixRequest`, one edit), `a11y_heading_proposal`, `a11y_reading_blocks`, `a11y_region_picture`, `a11y_save_report`, `a11y_words` (the engine's sentences and `PDFUA_HUMAN_CHECKS`) |
| `app/shell-core/src/a11y/` | `region.rs` (the `RegionRenderer` over the worker: the page is drawn once per check and areas cut out of it), `fixes.rs` (`FixRequest` as the window sends it, and `apply_fix`), `blocks.rs` (where each tagged block of a page is and its first words, for the numbered reading order), `tests.rs` |
| `app/ui/src/lib/a11y/` | `a11ySession.svelte.ts` (the report, the wizard, the outlined spot and the numbered blocks; checks again after each fix, and after undo while the wizard is open), `A11yMarks.svelte` (the page overlay), the pure modules `findings.ts`, `wizard.ts`, `ladder.ts`, `languages.ts`, `contrast.ts`, `tokenAudit.ts` (all tested in `a11y.test.ts`) |
| `app/ui/src/lib/inspector/AccessibilityTab.svelte`, `inspector/accessibility/*` | the tab: Check This Document, Fix Problems… (the wizard and one small form per `FixKind` in `fixes/`), Mark as PDF/UA, Save Report… |

- **The wizard follows the file.** Steps are keyed by rule and place
  (`findingKey`: object numbers survive lopdf's save). After a fix the
  document is checked again: a step whose finding is gone is fixed, one
  that came back after Undo is open again, new ones (pictures that need
  alt text once tags exist) are added at the end.
- **Block places are worked out in the shell.** The engine proposes a
  reading order as a list of tags without places; `blocks.rs` follows each
  tag to its mcids and walks the page's content to find what they draw.
  Text widths are estimated from letter counts.

## Editing page content in place (Edit)

The engine is `core/edit` (see its README, especially "Keeping the original
font" and "Known limits"). The app's parts:

| Where | Job |
|---|---|
| `app/shell/src/edit_commands.rs` | one command per operation through `state.edit`: `edit_replace_text` (with `fit` for Shrink to Fit), `edit_insert_text`, `edit_delete_chars`, `edit_delete_text`, `edit_set_text_style`, `edit_add_text` (a standard font or a TrueType file from the font library), `edit_add_image` (a file or pasted bytes), `edit_replace_image`, `edit_move_image`, `edit_resize_image`, `edit_rotate_image`, `edit_delete_image`, `edit_restack_image`; reading: `edit_page_objects`, `edit_save_image`, `edit_font_face` (raw bytes) |
| `app/shell-core/src/edit/` | `fonts.rs` (the embedded TrueType or OpenType file of some text, only when its own cmap draws every letter), `fit.rs` (Shrink to Fit: words shared over the paragraph's lines, then made smaller to the old width, on a copy), `pictures.rs` (png or jpeg only, 50 MB at most, plain refusals), `mod.rs` (`block_matching`: finding a block again by place), `tests.rs` |
| `app/ui/src/lib/edit/` | `editSession.svelte.ts` (the tools, the editor draft, notices), `pictureTools.svelte.ts` (the selected picture and every picture change, held by the session as `pictures`), the pure modules `blocks.ts` (hit testing, finding things again), `editorGeometry.ts` (the editor's box from the baseline), `textDiff.ts` (typing to the smallest change), `editSentences.ts`, `cssFonts.ts` (the editor's font, FontFace loading, metrics), `pageObjectsStore.svelte.ts`, all tested in `edit.test.ts`; the page parts `EditOverlay.svelte`, `InPlaceEditor.svelte`, `EditNotice.svelte`, `EditChrome.svelte` (paste and drop); `editApi.ts`, `editTools.ts`, `editCommands.ts` |
| `app/ui/src/lib/inspector/EditTab.svelte`, `inspector/edit/*` | the tab and its sections; `FontMenu.svelte` is the one place a font picker plugs in |

- **Ids are only valid until the next edit of a page.** Every command
  answers the page's objects as they are after the change, and
  `PageObjectsStore.accept` keeps them for the revision that edit made, so
  nothing is fetched twice and no stale id is ever sent on purpose. An id
  the engine no longer knows comes back as `stale: true` with the fresh
  objects (not as an error); the session finds the same block or picture
  again by place (`matchingBlock`, `matchingImage`) and tries once more.
- **Typing is one change.** The editor keeps what is typed and sends it
  when it is left (click elsewhere, Ctrl Enter); `planTextChange` makes it
  the smallest engine call: an insert or delete inside one line keeps that
  line's own glyphs and spacing, a rewrite of one line re-sets only it, and
  only several changed lines re-set (and re-wrap) the paragraph.
- **The editor sits on the baseline.** Its top is worked back from the
  first baseline with the editor font's measured ascent and descent, so it
  lines up at any zoom. The font is the embedded one when the web view can
  load it (a subset with private codes or missing tables isn't), else the
  font of that name on this computer, else one of its kind. What the
  engine really wrote comes back as `FontNotice`s, said beside the text.
- **Paragraphs and pictures are targets, not a page tool.** While Edit
  Text and Images is on, `EditOverlay` lays a `data-page-ui` button over
  every visible block and picture; the session's page tool only hears
  presses that miss them (it commits the editor). Add Text is a
  `pointTool`, Add Image a `rectangleTool`. The viewer's one active tool is
  `protect ?? fillSign ?? edit ?? annotate`.

## Text recognition (Scan)

The engine is `core/ocr` (see its README). The app's parts:

| Where | Job |
|---|---|
| `app/shell/src/ocr_commands.rs` | `OcrState` (inside `AppState`: the engine, runs, words, image runs); `ocr_status`, `ocr_scan_overview`, `ocr_start` / `ocr_job_status` / `ocr_cancel` / `ocr_discard` / `ocr_apply`, `ocr_remove_text`, `ocr_overlay`, `ocr_correct_word` |
| `app/shell/src/ocr_images_commands.rs` | New from Images: `ocr_images_to_pdf` / `ocr_images_status` / `ocr_images_cancel` / `ocr_open_new_document` |
| `app/shell/src/content_menu.rs` | the macOS Content menu and File > New from Images… |
| `app/shell-core/src/ocr/` | `engine_folder.rs` (where tesseract is, and the missing message), `job.rs` (one run through a `PageReader`, progress, cancel), `jobs.rs` (the runs going on, one per document), `render.rs` (a page at 300 dpi from the worker's tiles, as gray), `overview.rs`, `summary.rs`, `words.rs` (the overlay's words and correcting one), `tests.rs` (a fake engine) |
| `app/ui/src/lib/ocr/` | `scanSession.svelte.ts`, the pure modules `ocrJob.ts` (the run's state machine), `pageRange.ts`, `languages.ts`, `ocrSentences.ts`, `imagesToPdf.ts`, `newDocuments.ts` (tested in `ocr.test.ts`); `ScanBanner.svelte`, `RecognizedOverlay.svelte`, `NewFromImagesSheet.svelte`; `ocrApi.ts`, `ocrTools.ts`, `ocrCommands.ts` |
| `app/ui/src/lib/inspector/ScanTab.svelte`, `inspector/scan/*` | the tab and its sections |

- **The engine is a resource.** `tauri.conf.json` bundles
  `vendor/tesseract` as `tesseract/`; at run time it is looked for in
  LETTERS_TESSERACT_DIR, the resources, beside the program, then the
  source tree, the way pdfium is. Missing, the tab says how to fix it (in a
  developer build: run `tools\fetch-tesseract.ps1`).
- **A run never touches the document until it is done.** `ocr_start`
  checks the languages, registers the run and reads the pages on its own
  thread, one at a time (`RenderedReader`: tiles from the worker's normal
  queue, so the screen keeps drawing, then `recognize`). The ui asks
  `ocr_job_status` about three times a second. Cancel flips the run's
  `CancelFlag`, which stops tesseract at once. A finished run waits until
  the ui calls `ocr_apply` through the viewer's change queue: one
  `state.edit` adds every page's layer, so it is one undo step, and a
  failure part way leaves nothing behind.
- **Words for the overlay.** Right after a run the shell keeps
  tesseract's words (with confidence) per document and page; they are
  used only while they still match the invisible text on the page (read
  back with letters-edit's `page_objects`), so after an undo, or in a file
  opened later, the overlay shows the page's own invisible words without
  confidence. Correcting a word writes the page's layer again from the
  corrected words (letters-ocr has no call that changes one word).
- **New from Images** writes the pdf into the app's cache, opens it like
  any document, marks it unsaved and deletes the cache file, so nothing
  lands in the person's folders; `newDocuments.ts` makes Save ask where,
  starting beside the first picture.

## Conversion and export

The engines are `core/convert` (pictures, text, web pages, spreadsheets,
slides, the structure analysis, office files to pdf) and `core/exact`
(pixel-exact word documents). The app's parts:

| Where | Job |
|---|---|
| `app/shell/src/export_commands.rs` | `export_capabilities_list`, `export_document` (one export on its own thread, "export-progress" events, Cancel), `export_cancel`, `convert_office_to_pdf`, `show_office_as_pdf`, and "Open PDF as Document…" (`open_pdf_as_document_window`, `take_open_as_document`, `open_pdf_file_as_document`) |
| `app/shell-core/src/export/` | `request.rs` (the job and the outcome), `choices.rs` (the engine's capability list with the entries built here marked available), `run.rs` (the right engine per format), `easy_edit/` (the structure model mapped onto the office model), `exact.rs` (the adapter to core/exact), `renderer.rs` (whole pages from the app's pdf worker, for both engines), `summary.rs` (the result line from each engine's report), `office.rs` |
| `app/ui/src/lib/export/` | `exportChoices.ts` (capabilities to the format list, the options, checking them, the request; tested), `ExportSheet.svelte`, `exportApi.ts`, `conversionCommands.ts` and `conversionTools.ts` (the commands and words of this stage, joined into `commands.ts` and `tools.ts`), `openByKind.ts` |

- **An export reads the document as shown.** The working copy (edits
  included) is copied to the temp folder first, so closing the document
  while an export runs can't pull the file away; the copy is deleted after.
- **"Easy to Edit" Word and OpenDocument files** are built here, not in an
  engine: `analyze_structure` gives blocks in reading order, and
  `easy_edit` maps them onto `letters_office::model` as core/convert's README
  describes: headings with the document's heading styles
  (`outline::heading_style`), paragraphs with runs whose marks are only what
  differs from the body text (the body font and size live in Normal),
  bulleted items sharing one list and each numbered list restarting, tables
  with grid widths, spans, header rows, shading and ruled edges, pictures
  copied out with letters-edit at the size they had on the page, footnotes
  as real notes whose reference replaces the raised marker, the most certain
  repeated header and footer as real ones (page numbers as fields), and a
  section per run of pages of one size and column count, with margins read
  from the text's extent (kept between a quarter inch and two inches).
  `write_docx`/`write_odt` write it.
- **Exact Match Word** goes through `export/exact.rs` only. core/exact
  starts and ends its own private Word; the adapter passes a cache folder
  in the app cache (measured Word facts are reused), a work folder in the
  temp folder (Word can't open files in the app's cache), `verify` when
  Word is installed, our renderer, progress and the stop switch. The
  outcome carries the engine's verification sentence, the measured numbers
  for "How close is it?", and each font stand-in with its reason. Without
  Word the result line says it was not checked on this computer.
- **"Open PDF as Document…"** opens a new window (the window that asked
  keeps its document): the pdf's path is remembered by the shell, the new
  window's `App.svelte` asks `take_open_as_document` once, and the writer
  opens the in-memory model through `WriterSessions::open_converted`
  (the one hook added to the writer), named for a free ".docx" beside the
  pdf.

## AI Tools

The engine is `core/ai` (see its README: plan, disclosure, acknowledge,
run; the disclosure can't be bypassed).

| Where | Job |
|---|---|
| `app/shell/src/ai_commands.rs` | `ai_status` (the panel header), `ai_plan`, `ai_run` (streams "ai-progress"), `ai_stop`, `ai_find_offline` (Find a Topic, no ai), `ai_test_connection`, `ai_save_key`, `ai_has_key`, starting and stopping a model server |
| `app/shell-core/src/ai/` | `document_text.rs` (the structure analysis as pages of paragraphs with rectangles: one per list item and table row, so a citation outlines what it quotes), `desk.rs` (plans wait under their disclosure id for Send; "Don’t ask again" is checked here too), `consent.rs` (one provider and one document per yes, in memory only), `keys.rs` (the system credential store), `connection.rs` (Test Connection's one fixed sentence) |
| `app/ui/src/lib/ai/` | `aiPanel.ts` (the request's steps as a state machine, the header line, an answer cut into text and citation chips; tested), `AiPanel.svelte`, `DisclosureSheet.svelte`, `aiApi.ts` |

- **Two gates.** The ui shows the engine's disclosure before anything
  that leaves the computer, and the shell refuses to run such a plan
  without that Send or a remembered yes for that provider and document.
  The engine itself refuses without its acknowledgement. A plan runs once.
- **Off means hidden.** The AI Tools button and panel appear only while
  the switch is on in Settings; the viewer reads the setting when its
  window comes forward. Find a Topic (Ctrl Shift F, or Topic… in Find)
  always works: it is the engine's offline word finder.
- **Keys.** `ApiKey` keeps its text private to core/ai, so a store outside
  the crate can't read a key back; Settings hands the typed text straight
  to `TypedKeyStore::save_text`, and the engine only ever loads.

## Settings

| Where | Job |
|---|---|
| `app/shell/src/settings_commands.rs` | the one Settings window (`window-settings`, so the usual capabilities apply), reading and writing the choices, clearing recovery copies and Recents, opening the system's Default Apps page |
| `app/shell-core/src/settings_store/` | `app-settings.json` in the app data folder: General, Files, Scanning, AI Tools (the engine's own `Settings`, which hold a key's name, never the key) and the model server's two files; unknown fields of a newer version are kept; tested that no key reaches the file |
| `app/ui/src/lib/settings/` | `settingsModel.ts` (reading any version, provider switching, what's missing, the privacy summary; tested), `SettingsScreen.svelte`, `AccountsSection.svelte`, `AiSection.svelte`, `settingsApi.ts` |

Settings that had a home before stay there: the name on comments
(`settings.rs`), the writer's typing switches and the Google Drive client
(`writer/settings.rs`, through `TypingToggles` and the writer's drive
commands), so nothing is stored twice. The privacy page and
`docs/PRIVACY.md` list everything that can leave the computer.

## Packaging and opening files by type

- **The installer** (`app/shell/tauri.conf.json`, `bundle`): NSIS per user
  (no administrator), file associations for pdf, docx, odt, ods, odp, rtf
  and the common pictures with names, roles and macOS content types, the
  resources (pdfium, the OCR engine, bundled fonts, document icons,
  `THIRD-PARTY-LICENSES.txt`). `app/shell/installer-hooks.nsh` adds each
  type's document icon and the "capabilities" Windows lists in Default Apps,
  under HKEY_CURRENT_USER, and removes them on uninstall. The running app
  never writes the registry; Settings opens Default Apps for the person.
- **Document icons** come from `tools/icon/make-document-icons.ps1`.
- **Licenses**: `tools/licenses/make-third-party-licenses.mjs` writes
  `THIRD-PARTY-LICENSES.txt` from `cargo metadata` and the ui's lockfile and
  flags anything that isn't MIT, Apache, BSD, ISC, Zlib, OFL, MPL or similar.
- **Where each type opens** is `shell-core/src/associations` (tested
  against the installer's list): pdfs in the viewer, docx and odt in the
  writer (rtf and doc are converted by the writer), pictures in New from
  Images (lib/ocr), spreadsheets and presentations as pdfs that Office or
  LibreOffice makes in the temp folder. Every file on the command line gets
  a window, through the single-instance path when Letters already runs; a
  mac's open-document event does the same (`associations.rs`).

## Search all documents, long documents and organising files

```
 ui: library/ (search screen)   reading/ (go to, history, bookmarks, progress)   organise/ (move, rename)
        |                               |                                             |
 shell: search_commands.rs      reading_commands.rs                           organise_commands.rs
        |                               |                                             |
 shell-core: library_search/ (job, sources, settings)  reading/ (store, labels, outline, versions)  organise/
        |
 core/search: index, query, search, expand, snippet, extract, jump (no pdfium: pdf text comes through a trait)
```

- **The index (`core/search`).** Written here rather than taken from
  tantivy: a few hundred lines cover what the app needs, it builds on the
  gnu toolchain, and it adds nothing to the program's size. A passage is a
  pdf page or a word paragraph. Words are folded (case, accents) and kept
  as posting lists with varint gaps; passage text lives in an append-only
  `passages-N.bin` and is read only for the results shown. `index.bin` is a
  checkpoint written whole and renamed into place, so a crash leaves the
  last checkpoint, and the files after it are simply read again. Titles and
  headings are indexed under a mark (`\u{1}word`) for the heading boost and
  "Titles and Headings". A query is answered in two rounds: bm25 over the
  word lists (each word widened to its stem forms, a prefix while typing,
  and typo matches when it is not found), then the best 300 passages are
  read to check phrases, reward closeness and cut snippets.
- **The job (`shell-core/library_search`).** One thread: lists the files
  (recents, library folders, opted-in pinned folders, less excluded
  folders), drops the gone ones, reads only changed ones (size and time,
  then a checksum of the first and last 64 kb), rests after each file and
  checkpoints every 50 files or 5 seconds. PDFs are read through
  `search_commands::EnginePdf`, which uses the app's one pdf worker, so
  tiles still come first. No password is ever used: a locked file is
  indexed by name and noted.
- **Opening at a hit.** `lib/library/arrival.ts` leaves "page and query"
  in local storage for the path; the viewer takes it when it opens and
  goes to the page with the query in its own Find bar. This works for a
  new window too.
- **The page finder.** `core/search/src/jump.rs` and its mirror
  `lib/reading/jump.ts` (same tests) read what the reader types. Page
  labels come from the pdf's `/PageLabels` (`shell-core/reading/labels.rs`).
- **Reading state.** `shell-core/reading/store.rs` keeps the last
  position, bookmarks, seconds per page and word goals per document in
  `reading.json`; nothing changes the file except "Save Bookmarks into the
  PDF", which is an edit through `AppState::edit`.
- **Organising.** `shell-core/organise` never replaces a file silently
  (`Exists` comes back for the interface to ask), copies and checks the
  bytes before deleting across drives, and recycles through the system's
  recycle bin. After a move `organise_commands::follow` updates Recents,
  the index entry (no re-reading), reading records, versions, and open
  pdfs (`EditSession::follow_move`), and tells the windows.

## Code files (the code editor screen)

A small, separate screen for source and text files, kept out of the way of
everything else: it is loaded only when such a file opens, and it never
runs anyone's code.

- **Routing.** `App.svelte` sends a file the other screens don't take to
  `openInCode`; `code_open` (`app/shell/src/code_commands.rs`) reads it with
  `shell-core/code/text_file.rs`, which refuses binaries (a zero byte in the
  first 8000), keeps the byte order mark, encoding and most common line
  ending, and hands the editor text with `\n` breaks. Save writes the same
  kind of file back through `write_atomically`. Up to 2 MB a file gets every
  feature, up to 32 MB it is editable without highlighting or checks, up to
  128 MB read-only.
- **The screen** (`app/ui/src/lib/code`): `CodeScreen.svelte` is imported
  with `import()` from `App.svelte`, so CodeMirror 6 and its packages are a
  chunk of their own; `languageLoader.ts` imports one language package per
  file. `codeSession.svelte.ts` holds the path, the saved text (CodeMirror's
  `Text`, compared with `eq`), Save As, recovery copies
  (`shell-core/code/recovery.rs`, every 5 s while edited) and the warning
  before saving text that wasn't valid. The unsaved changes sheet comes
  through `writerGuard`, as for the writer.
- **Checks.** In-app checks (`checks/`) are pure functions from text to
  `Problem[]`; the compiler check (`compilerCheck.ts`) asks `code_check`,
  which finds tools (`shell-core/code/tools.rs`), copies the text into a
  fresh temp folder and runs only a syntax check (`checks.rs`:
  `gcc -fsyntax-only`, python's `ast.parse`, `javac -proc:none -d temp`,
  `node --check`) with a 15 s limit (`runner.rs`). The raw output comes back
  and `diagnostics.ts` reads it. `scheduler.ts` debounces and drops answers
  that a newer check has overtaken; the shell stops the older compiler when
  a newer check of the same window starts (`CheckGeneration`).
  `problems.ts` merges both lists for the squiggles, the gutter, the
  Problems panel and the status bar.
- **Documents.** `DocumentCodeTools.svelte`, placed by `App.svelte` beside an
  open pdf or word document, registers Export as Code File… and Check Code
  in This Document and loads their sheets on first use. Exports read the
  saved file (`shell-core/code/sources.rs`; a pdf through "Open PDF as
  Document…"'s structure) and write Markdown, HTML and plain text with
  letters-office's own writers, and LaTeX, reStructuredText, AsciiDoc, a
  json outline and csv tables with `shell-core/code/export`, which flattens
  the office model into a few simple blocks first (`simple.rs`). Code in
  documents is found by `shell-core/code/document_code.rs` (monospaced
  paragraphs, code styles, fenced blocks; monospaced pdf lines with their
  indent rebuilt) and checked by the same checks after `codeBlocks.ts`
  repairs swapped characters. "Open Block in Code Editor" hands the text to
  a new window as a temp file (`untitled.rs`), which the screen treats as
  untitled.

## Planned

In the order they will be built (each plugs into the edit pipeline above and
gets a tab in the Tools inspector): forms and signing,
OCR, redaction, compare, word-processor mode, conversion with the fidelity
harness, optional AI tools, the accessibility wizard, packaging and default-app
registration. Each will get its own crate under `core/`.

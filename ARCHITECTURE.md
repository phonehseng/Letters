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
| `engine.rs` | the only file that calls PDFium: open, page sizes, render one tile, read the outline, find text on a page |
| `outline.rs` | the shape of an outline entry and the limits that keep a damaged outline from looping |
| `search.rs` | matching a query against a page's characters, snippets, and turning a match into rectangles |
| `worker.rs` | the thread that owns the engine, the queue and the cache, and the handle other code uses to talk to it |
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

### `app/shell-core` (crate `letters-shell-core`)

The parts of the shell that need no window, kept apart so they can be unit
tested: the recent files list (`recents.rs`), pinned folders (`pinned.rs`),
reading tile and thumbnail addresses (`address.rs`), and finding a file on the
command line (`startup.rs`).

### `app/shell` (crate `letters-shell`, program `letters`)

The desktop program. Thin glue, no logic of its own worth testing.

| File | Job |
|---|---|
| `lib.rs` | starts the app, wires plugins, closes a window's documents when the window closes |
| `state.rs` | what the running app remembers: the worker, which window owns which document, recents |
| `commands.rs` | the functions the interface can call |
| `tile_protocol.rs` | serves tiles and thumbnails to `<img>` tags over a private `tile://` address |
| `windows.rs` | creates windows |
| `menu.rs` | the macOS menu bar. each item sends a `menu-command` event to the window in front |
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
| `lib/PageView.svelte`, `lib/SearchHighlights.svelte` | one page with its tiles, and the match highlights over it |
| `lib/Sidebar.svelte`, `lib/Thumbnails.svelte`, `lib/Outline.svelte` | the left sidebar and its two modes |
| `lib/Home.svelte`, `lib/FolderList.svelte` | the Recents screen and its pinned folders |
| `lib/HelpPanel.svelte` | the Explain side panel |
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

## Planned

In the order they will be built: page tools, annotations, forms and signing,
OCR, redaction, compare, word-processor mode, conversion with the fidelity
harness, optional AI tools, the accessibility wizard, packaging and default-app
registration. Each will get its own crate under `core/`.

# Changelog

All notable changes to Letters are written here, newest first.

## Unreleased

### Stage 1: viewer and file opener

Works:

- Open a PDF from the file picker, by dropping it on the window, by passing it
  on the command line, from Recents, or from a pinned folder.
- Recents screen with a picture of each document's first page, page count,
  when it was opened, and removing an entry.
- Pinned folders on the Recents screen: pin a folder and its PDFs are always
  listed, newest first. One search box filters recents and folder files.
- Scrolling viewer that only draws what is on screen: pages are cut into
  512 pixel tiles, rendered on demand, and kept in one cache with a fixed
  24 MB budget shared by every window.
- Zoom in, zoom out, fit width (the default, and it follows the window),
  Ctrl + mouse wheel, jump to a page, night mode.
- Two Pages view: pages side by side in pairs, like an open book.
- Sidebar with page thumbnails (only the ones in view are drawn) and the
  document's outline, with the current page or section marked.
- Find: every match is highlighted on the page, with a count, next and
  previous, and a 500 match limit. Works on rotated pages and across line
  breaks.
- Several windows, each with its own document, all inside one process that
  shares one PDF engine and one tile cache. Starting the app a second time
  adds a window to the running one.
- Password-protected PDFs ask for their password.
- Every button has a tooltip and an entry in the Explain panel saying when to
  use it and what it does to the file. Every action has a keyboard shortcut.
- The toolbar drops its least important items first on a narrow window; they
  stay reachable by shortcut.
- Follows the system's dark mode, reduced motion, reduced transparency and
  increased contrast settings.

Not verified or not done:

- macOS: nothing has been built or run there. The macOS menu bar is written
  but has never been run.
- Screen readers (Narrator, VoiceOver) have not been tried on the sidebar,
  outline and find controls.
- Very long outlines (thousands of entries) are not virtualised.
- Opening .docx, OpenDocument and image files, and registering as the default
  app, belong to later stages.

Measured on Windows 11, release build, 500 page test file:

- Cold start to first page drawn: about 0.6 to 0.8 seconds.
- The Letters process itself: about 38 MB at start, about 93 MB after
  scrolling through 280 pages.
- The system web view that draws the interface adds much more: about 345 MB
  across its processes with one window, rising to about 570 MB after heavy
  scrolling, and about 70 MB more for a second window. Bringing this down is
  open work.

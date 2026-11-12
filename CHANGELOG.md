# Changelog

All notable changes to Letters are written here, newest first.

## Unreleased

### Stage 2: the editing foundation

The file can change for the first time. Every later tool (page tools,
annotations, redaction, forms) plugs into this pipeline; see "How an edit
works" in ARCHITECTURE.md.

Works:

- Opening a document makes a working copy in the app's cache; the viewer
  shows the copy. Edits change only the copy until Save.
- Every version is its own file that is never rewritten, so a failed edit
  leaves the document exactly as it was, and the engine can never read half
  an edit.
- Undo and redo, up to 30 steps each and at most 512 MB of snapshots each,
  oldest dropped first. Undoing back to the saved version clears "Edited".
- Save writes a temporary file beside the original and renames it over the
  original, so a crash never leaves half a file. A missing or read-only file
  is refused before anything is written, with a message that suggests Save
  As. Save As adds ".pdf" if it was left off and the document then belongs
  to the new file. Recents and its picture follow each save.
- Rotate Left and Rotate Right for the current page, the reference edit
  later tools copy (`app/shell-core/src/rotate.rs`). It respects a rotation
  inherited from the page tree.
- The document's name in the toolbar is now the document menu: Save,
  Save As…, Undo, Redo, Rotate Left, Rotate Right. Disabled items are greyed
  and their tooltip says why ("Nothing to undo"). While there are unsaved
  changes the title shows "— Edited" in the secondary colour, and so does
  the window title.
- Shortcuts: Ctrl S, Ctrl Shift S, Ctrl Z, Ctrl Shift Z (and Ctrl Y), Ctrl L,
  Ctrl R. In a text field Ctrl Z still undoes the typing. The macOS menu bar
  has Save and Save As… under File, our own Undo and Redo under Edit (the ui
  sends them to a text field or to the document), and a Tools menu.
- After any change the page sizes, layout, tiles, thumbnails, outline and
  find results all refresh, and the reader stays at the same spot of the
  same page.
- Unsaved changes are never lost silently: going back to Recents or closing
  the window asks "Do you want to save the changes made to “name.pdf”?"
  with Don’t Save, Cancel and Save (Enter saves, Escape cancels). A failed
  save keeps the document open and says what to do.
- Closing a document or its window deletes its working copies; copies left
  by a crash are removed at the next start.
- The Explain panel describes every new command, and says exactly when the
  file on disk changes.

Not verified or not done:

- macOS: the new menu items have not been run on a Mac.
- The menu bar's Undo and Redo are always enabled; when there is nothing to
  undo they do nothing.
- Quitting the app from the macOS app menu does not ask about unsaved
  changes yet; closing a window does.
- PDFs locked with a password can be read but not edited.
- Opening copies the whole file into the cache first, which takes a moment
  for very large files. Each edit loads and writes the whole working copy
  with lopdf, so an edit of a several hundred megabyte scan is slow.
- Save replaces the file, so file system details of the original (its
  permissions list, alternate data streams) are not carried over.

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

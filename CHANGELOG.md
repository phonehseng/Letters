# Changelog

All notable changes to Letters are written here, newest first.

## Unreleased

### Stage 2: page tools

Works:

- A Tools inspector on the trailing side (toolbar button Tools, Ctrl Alt I),
  with a segmented control for its tabs. The first tab, Pages, holds eleven
  tools as disclosure sections: Rotate, Delete Pages, Insert (blank pages or
  pages from another PDF), Extract, Split (by ranges, every N pages, by
  chapter, by file size), Merge (a list to arrange and name), Crop, Page
  Numbers, Header & Footer, Watermark (text or picture, in front or behind,
  with an opacity slider) and Remove (page numbers, headers and footers, or
  watermarks that Letters added). Each open section explains new words,
  shows its few settings and one action button, and says under the button
  exactly what will happen to which pages and to the file. Defaults make the
  common case one click.
- Later stages add a tab with one registry entry and one component (see
  "Adding a tab to the Tools inspector" in ARCHITECTURE.md).
- Selecting pages in the thumbnails: click, Ctrl-click (Command on a Mac),
  Shift-click, Ctrl A, the arrows and Shift with the arrows. Tools act on the
  selection, or on the page being read when nothing is selected, and say which.
- Reordering by dragging thumbnails, one or many, with an insertion line and
  the list scrolling itself at its edges; it works from layout numbers, so a
  500 page document reorders as smoothly as a short one. Alt Up and Alt Down
  move the selected pages from the keyboard.
- Delete deletes the selected pages (Undo brings them back); deleting every
  page is refused with a plain sentence. A right-click menu on thumbnails
  (and the Menu key or Shift F10) offers Rotate Left and Right, Delete,
  Extract…, Insert Blank Page After and Insert from PDF….
- PDFs dropped from the system onto the thumbnails are inserted where the
  line shows.
- Crop shows a rectangle on the page with eight handles (or arrow keys, Shift
  to resize), with number fields and "This Page / Selected / All Pages".
  The text explains that cropping hides and doesn't remove, and points to
  redaction under Protect.
- Extract, Split and Merge make new files and leave the open document alone.
  Split never replaces a file in the chosen folder: taken names get " (2)",
  " (3)", and the message lists the files made. Extract and Merge offer to
  open the result. Password-locked PDFs given to Insert or Merge get the
  page tools' message.
- macOS: a Pages menu replaces the temporary Tools menu, and Tools is in View.
- Explain follows the inspector: with Tools open it lists the open tab's
  tools first. Sidebar, Tools and Explain share the window by one rule: the
  document keeps at least 26rem, and the panel opened longest ago gives way.
  Below 46rem the inspector floats over the document.
- Fixed: the type scale was 19% too small everywhere. The body font was set
  on `:root`, which made 1rem 13px instead of 16px, so the footnote style
  came out at 8px. Text and controls are now the sizes DESIGN.md gives.

Measured on Windows 11, debug build, 500 page file: moving a page with Alt
Down took about 130 ms from key to new state, a drag across 150 pages about
140 ms from drop to new state, and undo about 250 ms. Only 8 of the 500
thumbnails were built at any time.

Not verified or not done:

- Dropping a file from the system onto the thumbnails: the logic is unit
  tested, but a system drag can't be driven from the test scripts, so it has
  not been tried by hand.
- The system's file panels (open, save, folder) were answered by the test
  scripts, not clicked, so their own behaviour (the "replace?" question,
  for one) was not exercised.
- macOS: the Pages menu has not been run on a Mac. Its Alt Up and Alt Down
  keys would take Option-arrow away from text fields there while the menu
  is in front.
- Merge uses the open document as last saved; the inspector says so when
  there are unsaved changes.
- Crop numbers are in points only.

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

# Changelog

All notable changes to Letters are written here, newest first.

## Unreleased

### Stage: code files

A small code editor for the code and text files students hand in with
their documents, checks that say how much to trust them, exports from
documents to code-friendly formats, and a check of code found inside
documents. Nothing is ever run: compilers already on the computer are
only asked for a syntax check.

Measured (debug build, this computer): a 20.3 MB, 295,375-line
JavaScript file opens in 2.2 to 2.4 s including the app's start; a
keystroke shows in 4 to 5 ms and jumping to the middle takes 5 ms; saving
it takes 0.45 to 0.59 s. Opening an HTML file from Recents in the same
window adds 26 MB of working set (letters.exe and its web view together)
and 3.8 MB of JavaScript heap. The code screen is 1.18 MiB of JavaScript in
24 chunks loaded only when a code file or one of the two sheets opens (one
language package per file).

Works:

- **The code screen**, routed like the viewer and the writer: HTML, CSS,
  JavaScript/TypeScript, JSON, Markdown, XML, C, C++, Python, Java, Rust,
  SQL, shell, YAML, TOML and plain text, each language loaded on its own;
  any other file that is text opens as plain text, anything else is
  refused with a sentence. CodeMirror 6: highlighting, line numbers,
  bracket matching, auto-indent, folding, multiple cursors, find and
  replace with regexp (Ctrl F, Ctrl H), Go to Line (Ctrl G), Comment
  (Ctrl /), Wrap (Alt Z), indentation detected and changeable, light and
  dark following the app, a cursor that doesn't blink with reduced motion.
- **Files stay the files they were**: UTF-8 with or without a byte order
  mark, UTF-16, CRLF, LF or CR kept on save; mixed endings saved as the
  most common one (the status bar says so); invalid UTF-8 asks before
  saving. Safe save, "— Edited", the unsaved changes sheet, recovery copies
  every 5 s, Recents (with "C file", "Python file"...).
- **Large files**: up to 2 MB every feature; up to 32 MB editable without
  colours or checks; up to 128 MB read-only; larger refused with a
  sentence.
- **Checks inside Letters**: JSON (exact place and message), HTML (tags not
  closed or closed out of order, duplicate ids, missing alt text, a block
  inside a paragraph, a link in a link, li outside a list), XML
  well-formedness, CSS (braces, colons, missing semicolons, properties the
  browser doesn't know), JavaScript (acorn), YAML, TOML, Markdown (heading
  levels, empty links, missing anchors, open fences); for C, C++, Java,
  Rust, Python, shell and SQL the bracket, quote and comment balance,
  Python's indentation, and the highlighter's own error marks.
- **Compiler checks** with gcc or clang (-fsyntax-only), Python
  (ast.parse), javac (-proc:none into the temp folder) and node --check,
  found on PATH and in the usual per-user folders, run on a temp copy with
  a 15 s limit; includes are read in place. "Checked by gcc 16.2" or
  "Basic check only: no C compiler (gcc or clang) found on this computer."
- **Problems**: squiggles, gutter marks, a Problems panel (Ctrl Shift M,
  F8 for the next) with line, message, who said it and a plain hint for
  the commonest C, HTML and Python mistakes; counts in the status bar;
  re-checked after a pause in typing and on save, stale runs dropped and
  the older compiler stopped.
- **Open in VS Code** at the cursor's line when VS Code is installed
  (otherwise the button is hidden and Explain says why), **Show in
  Folder**.
- **Export as Code File…** from the viewer and the writer (document menu,
  Ctrl Alt X, the Code menu on a mac): Markdown, HTML and plain text through
  letters-office, and new LaTeX, reStructuredText, AsciiDoc, JSON outline
  and CSV tables writers; pictures go to an "images" folder beside it.
- **Check Code in This Document** (Ctrl Alt K): code in monospaced fonts,
  code styles and ``` fences in word documents, and monospaced lines in
  PDFs with their indent rebuilt; the language guessed and changeable;
  curly quotes, non-breaking spaces, dashes and ligatures flagged and
  repaired before checking; Open Block in Code Editor opens an untitled
  window.

Not done:

- TypeScript has no checker beyond the bracket balance and the
  highlighter's marks (tsc is not run). Rust is not compiled (rustc
  --emit=metadata was judged too heavy).
- Jupyter notebooks and Org Mode exports: listed as "Coming later".
- Exports read the document as saved, not unsaved edits in the writer.
  Lists set only by a paragraph style ("List Bullet") come out as lists in
  LaTeX, reST, AsciiDoc and JSON but as plain paragraphs in Markdown, whose
  writer is letters-office's.
- The writer's own document menu has no entry for the two commands; they
  are reached with Ctrl Alt X and Ctrl Alt K there (and the Code menu on a
  mac).
- Untitled blocks have no recovery copies.

Limits:

- Compiler checks can't be put in a network sandbox here; they read only
  the copy and headers, and are given no network flags or build steps.
- Code found in a PDF is a hint: characters, spaces and line breaks can be
  lost when code goes into a document, and the sheet says so first.
- The macOS Code menu is written but has not been run on a Mac.

### Stage: search and long documents

Search every document at once, down to the page; find any page of a long
book; keep your place and your bookmarks; organise files without leaving
Letters. The index stays on this computer.

Measured: a generated 5,000 document library (21.1 million words) indexes
in 19.3 s in a release build, takes 206.7 MB on disk and about 51 MB of
memory; queries as you type take 19.8 ms at the median and 31.6 ms at p95.
In the app, typing in a 150,586 word novel paints in 34 ms (p50) and 40 ms
(p95), measured as two animation frames after the key.
Works:

- **Search All Documents** (Ctrl Shift G, or Everywhere beside the Recents
  search field): results as you type, grouped by document with their best
  pages and highlighted snippets; "phrases", -exclude, OR, prefixes while
  typing, english word forms, one-word typos ("Showing results for …"),
  accents and case ignored; chips for kind, date, folder and Titles and
  Headings; full keyboard; Enter opens the PDF at that page with the
  words in its Find bar; Ctrl Enter opens a new window.
- **The index**: Recents, library folders and opted-in pinned folders,
  recursively, less "Don't Index" folders; PDFs per page (scans noted as
  needing text recognition), DOCX/ODT/RTF per paragraph with headings and
  an estimated page, text and Markdown. Incremental, throttled, pausable
  between pages, safe across crashes (checkpoints), locked files by name
  only (no password is ever stored). Pause, Resume, Rebuild and Delete in
  the screen's Settings panel and in the Settings window's new Search
  section.
- **Go To** (Ctrl G): page numbers, printed labels ("xii", "A-3"),
  percentages, "+5", "ch 3", heading and bookmark names, "last".
  **Back and Forward** (Alt Left and Right) through jumps.
- **Reading**: reopens where you left off with "Start from the
  Beginning" one click away; bookmarks (Ctrl D) kept on this computer;
  a progress line with time left at your own measured pace; a page
  scrubber on long documents; Show All Matches lists every Find match.
- **Organising** in Recents: Ctrl- and Shift-click to choose; Move to
  Folder… (Ctrl Shift M), Copy to…, Rename (F2), Show in File Explorer,
  Remove from Recents, Move to Recycle Bin (Delete, asks first); name
  clashes offer Keep Both, Replace or Skip; Undo moves files back; Recents,
  the index, bookmarks, positions, versions and open PDFs follow a move.
- Engine pieces for the writer: goals and streak maths, section word
  counts, versions saved in the data folder (shell commands exist).

Not done:

- The writer's side through hooks: goals in its status bar, focus mode,
  typewriter scrolling, session timer, the versions list, section counts
  in its outline, and opening a word document at a search hit's
  paragraph (the hit opens the document at the top).
- Reader (reflow) view, split view of one document.
- "Save Bookmarks into the PDF" has its command and tests but no button yet.
- Dragging tiles onto folders, New Folder in the interface, Tidy Up.
- Find in all open documents.

Limits:

- Ctrl Shift F stays Find a Topic; Search All Documents is Ctrl Shift G.
- The index stores passage text uncompressed: about 10 bytes per word of
  the library on disk.
- Moving a word document that is open in the writer does not move its
  session; the writer would save to the old place.

### Stage: compare

Two versions of a PDF side by side or laid over each other, with every
change in one sentence, and a report. Neither file changes.

Works:

- **Entry points**: Compare with Another Version… in the document menu
  (the open document, unsaved changes included, is the newer version) and
  Compare Two PDFs… on Recents; both start at a screen naming the older
  and newer version with Choose…, Swap and the options. Two PDFs dropped on
  it fill both. A locked PDF gets the engine's own sentence ("This PDF is
  locked with a password. Unlock it first, then compare again.").
- **Its own screen**, like the viewer: the document underneath stays open
  and untouched (scroll, tools, marks), Back returns to it. The comparison
  runs in the background with a progress bar, the stage in words and Stop.
- **Side by Side**: old left, new right, one column of pages each, drawn
  with the viewer's tiles; the columns scroll together through the
  engine's scroll map (Link Scrolling turns it off); a page only one side
  has is a dashed placeholder. Changes are soft tints with a word on each
  ("Added", "Removed", "Changed", "Moved", "Formatting", "Layout",
  "Picture", "Page added"); picking one outlines it and its partner and
  scrolls both sides to them.
- **Overlay**: the engine's overlay picture per page pair, a slider from
  Old through both to New, and Flash (with reduced motion, each press
  shows the other version; nothing moves by itself).
- **Changes sidebar**: "6 changes: 3 text, 1 formatting, 1 pictures, 1
  pages", filters by category, Previous and Next Change (Alt ↑ ↓), every
  change under its page as a listbox; "These two documents are the same."
  for identical documents written with different bytes.
- **Options**: ignore spacing and hyphenation (on), compare formatting,
  layout and pictures, then Compare Again. **Save Report…** as PDF (with
  page pictures drawn by Letters' engine), web page or plain text; the PDF
  opens in the viewer.
- Zoom and Fit Width; night mode never applies here.
- macOS File menu items (never run on a Mac).
- **Tests**: 32 UI tests (grouping, filters, stepping, the scroll map with
  the engine's own cases, the virtual list, the overlay, progress, setup,
  the report); 8 shell-core tests against pdfium (progress after every
  page, a changed word, identical files, Stop, everything closed and
  deleted afterwards, a locked file, stitched tiles, pair pictures and a
  report the engine opens).
- **Proved in the real app** (debug build, `cdp-compare/compare.mjs`, 25 of
  25 checks): two versions with a changed word, an inserted sentence, a
  moved paragraph, a bigger title, a replaced picture and an added page
  give exactly those six changes (936 ms); picking the move scrolls both
  sides; linked and unlinked scrolling; the slider and Flash; the report
  saved and opened in the viewer; identical files; a 300 page pair (2.3 s,
  progress shown, 6 pages in the page at a time, worst scroll frame 3 ms).

Not done, and limits:

- The engine reports a paragraph that moved away from a place where new
  text was then inserted as "Changed … to …" plus "Added …", not as a move
  (a replaced passage is never offered to the move finder; core/compare
  `text_diff.rs`). The proof document inserts its sentence elsewhere.
- Marks on the pages are for the pointer; the keyboard and screen readers
  use the Changes list. No connector line is drawn between partners.
- Pages are drawn at the zoom's scale with the shared 24 MB tile cache, so
  comparing evicts the viewer's tiles.

### Stage: accessibility

A checker and fix wizard aimed at PDF/UA in a new Accessibility tab, and an
audit of the app's own interface.

Works:

- **Check This Document**: the engine's check, with contrast measured from
  pages drawn by Letters' own engine (each page drawn once per check). The
  engine's score statement first, then Must Fix, Should Fix and Check by
  Hand, each finding with its explanation, why it matters, what was found,
  Show on Page (an outline on the spot) and Fix….
- **Fix Problems…**: one problem at a time, "3 of 11", Back, Skip, Done.
  Forms: Add Tags Automatically (optionally with the language) and then
  "Look Over These Tags" with the guesses; the language by name; the
  title; alt text with the picture shown, or Decorative; the heading
  ladder (editable); table header rows and columns with the table shown;
  the reading order as numbered boxes on the page and a list moved by
  drag, buttons or Alt with the arrows; link, field and comment text;
  contrast with both colours, the ratio in words and the suggested colour;
  one-button fixes (tab order, tag links, bookmarks…). Every fix is one
  undo step; after each the document is checked again and the steps
  follow (Undo while the wizard is open reopens the step).
- **Mark as PDF/UA** only when the engine says every rule it can check
  passes, and only after the person ticks each of the engine's checks by
  hand. **Save Report…** as a web page or plain text.
- **Tests**: 21 UI tests (grouping, keys across checks, the wizard's steps,
  the ladder, languages, contrast maths, the app.css token audit); 6
  shell-core tests (the request shapes, fixes as single undo steps,
  headings, contrast from pdfium's pixels with the page drawn once, regions,
  reading blocks).
- **Proved in the real app** (`cdp-compare/a11y.mjs`, 22 of 22 checks): an
  untagged handout with two pictures, headings whose sizes start at the
  second level and skip one, and pale grey text (2.28:1, measured from the
  page's pixels) was checked; the wizard went from 1 of 5 to 8 of 8 (new
  problems joined after tagging) through Add Tags, language, title,
  contrast, alt text, headings and Decorative; a check again found none of
  them; Undo brought the last one back and Redo fixed it; Save and opening
  again kept every fix. Mark as PDF/UA stayed off, correctly: the standard
  fonts are not embedded, which the engine counts as must fix.
- **The app's own accessibility** (docs/ACCESSIBILITY.md): axe-core over 18
  screens in four colour modes went from 69 elements (light) and 186 (other
  modes) to 7 and 17, all in the writer except the sidebar's segmented
  control, whose token was then fixed too (not re-measured in the app). Fixed: the
  accent, accent text, red and secondary text tokens (6 failing pairs to
  0), a main landmark and heading in the viewer, Recents' toolbar in a
  landmark, a focus ring on the pages, Tab kept inside the unsaved changes
  sheet, 24 px Remove buttons on Recents.

Not done, and limits:

- Checking again after each fix checks the whole document (the engine has
  no partial check), drawing every page for contrast: fine for a handout,
  slow for hundreds of pages.
- Block places for the reading order are worked out in the shell with
  estimated text widths; boxes can be wider or narrower than the words.
- Text recognition fixes point to the Scan tab; nothing runs from here.
- Not heard through Narrator or VoiceOver; macOS menu items never run.
- Other areas' audit findings are listed in docs/ACCESSIBILITY.md, not fixed.

### Stage: direct editing

An Edit tab in the Tools inspector changes the page's own text in place
and its pictures, with core/edit.

Works:

- **Edit Text and Images** (Ctrl Alt W; Control Command W on a mac; also
  the macOS Content menu): every paragraph and picture of the pages on
  screen becomes a target; a paragraph shows a faint outline under the
  pointer or the keyboard focus, and Tab moves through them.
- **Typing in place.** Clicking a paragraph opens an editor exactly over
  it, on its baseline, at its size and colour, in its embedded font when
  the web view can load it (else the same font on this computer, else
  one of its kind), with the caret where it was clicked. Clicking
  elsewhere or Ctrl Enter makes the change as one undo step; Escape drops
  it. What was typed becomes the smallest engine change: an insert or a
  delete inside a line, a rewritten line, or the paragraph re-set.
- **Fonts are never changed silently.** A letter the font lacks is named
  under the editor while typing; afterwards a card under the text says
  which font drew it, from the engine's FontNotice.
- **Overflow**: when new text needs more lines than the paragraph had, the
  card says how much lower it reaches and offers Let It Grow or Shrink to
  Fit (the paragraph's own number of lines, smaller, as one undo step).
- **Text Style** (size, colour, a standard font) for the selected letters
  of a line or the whole paragraph; **Delete Paragraph**; **Add Text** in a
  standard font or a font on this computer from the font library, with one
  sentence on how it differs from a Text Box.
- **Pictures**: select with the shared frame (corners keep proportions),
  drag to move, drag a corner to resize, arrows to nudge (Shift 10 points,
  Alt resizes), Turn Left and Right, Bring to Front, Send to Back,
  Replace… (Fit, Fill or Stretch), Save Image As… (keeps a jpeg a jpeg),
  Delete, and Add Image… by choosing a file, pasting (Ctrl V) or dropping a
  png or jpeg on the page. Each is one undo step.
- **Stale ids**: every edit answers the page's objects with fresh ids,
  kept at once; an id that went out of date is answered quietly with the
  fresh page, and the session finds the paragraph or picture again by
  place and tries once more.
- **Tests**: 21 unit tests (`lib/edit/edit.test.ts`: hit testing, finding
  things again, editor geometry at several zooms, the smallest change,
  selections inside a line, the sentences, fonts); 10 shell-core tests
  (`edit/tests.rs`: finding a block again, Shrink to Fit keeps the lines
  and every word, a failed fit changes nothing, the embedded font handed
  over only when it draws the text, png and jpeg only, a stale id refused
  without an undo step). A script drove the real app (28 checks, all
  passed): a word edited in place in a PDF with an embedded Arial subset,
  Find found the new word, the next line's letters didn't move (0.000 pt);
  "é" typed (hint, then "The original font can’t draw “é”, so Arial from
  this computer was used for it."); text added where clicked; a picture
  moved, resized (proportions kept), nudged 10 points, replaced, turned,
  deleted, undone and redone; a picture placed with a click; saved,
  reopened and found again.

Not done:

- The font library has no `FontPicker` component yet, so the Edit tab
  uses a plain list (`inspector/edit/FontMenu.svelte` is where a picker
  would plug in). Existing text can only be switched to a standard font,
  which is all the engine offers.
- Rotated paragraphs get a rotated editor, but this was not tried in the
  real app.
- The editor uses the installed font when the embedded one can't be
  loaded by the web view (true of the reportlab subset in the proof), so
  it can differ slightly from the page while typing.
- With eight tabs the inspector's segmented control is crowded at its
  normal width (InspectorPanel.svelte isn't part of this stage).

Limits (from the engine, said in the tab under "What keeping the font can
and can’t do"): kerning inside changed words isn't kept, a paragraph that
grows doesn't push text below it down, CFF fonts count only the letters
already used, inline pictures can't be replaced or saved out, restacking
drops a clipping path. See core/edit/README.md.

### Stage: OCR

A Scan tab reads scanned pages with core/ocr (tesseract, run offline) and
adds an invisible text layer, so Find, selecting, copying and redacting by
text work on them.

Works:

- **The engine is bundled** (`tauri.conf.json` resources: vendor/tesseract)
  and found the way pdfium is; when it is missing the tab says how to fix
  it (a developer build: run `tools\fetch-tesseract.ps1`).
- **A quiet banner** on documents with scanned pages nobody has read:
  Make This Document Searchable opens the tab with those pages chosen.
- **Recognize Text** (Ctrl Alt O, Control Command O): the installed
  languages by name (the computer's own first; two at most, with the
  cost said), the pages (This Page, Selected, All, Scanned Only), and
  Straighten tilted pages, Clean up speckles, Detect sideways pages, each
  with a line. It runs on its own thread with per-page progress; Cancel
  stops tesseract at once and adds nothing; the end is one undo step with
  a summary ("Text found on 12 of 14 pages; 2 pages look blank."), the
  average confidence and the pages to check. The document stays usable.
- **Show Recognized Text** draws the words read over the scan, doubtful
  ones tinted; clicking one corrects it (the page's layer is written again
  from the corrected words, since the engine has no one-word call).
- **Remove Recognized Text.**
- **New from Images…** (Recents, File menu, the Scan tab; opening or
  dropping pictures offers it): PNG, JPEG or TIFF files in an order the
  person sets, Picture Size, Letter or A4, optionally made searchable. The
  PDF opens unsaved (nothing is written to their folders) and Save asks
  where, starting beside the pictures.
- **Tests**: 16 unit tests (`lib/ocr/ocr.test.ts`: the run's state machine
  with cancel, including a cancel that lands as the last page finishes,
  page choices, languages, the sentences, pictures in order); 9 shell-core
  tests (`ocr/tests.rs`) with a fake engine: progress in order, cancel
  stops before the next page, a failed page fails the run and leaves the
  document unchanged (and a half-applied layer is refused whole), a
  finished run is one undo step, one run per document, the summary, where
  the engine is looked for (and the `\\?\` resource paths tesseract can't
  use), the overlay's words and a correction. A script drove the real
  app (17 checks, all passed): three tilted, speckled scans made into a PDF with New from Images
  (reordered, opened unsaved, Save asked where), the banner, a run
  cancelled (stopped in under 3 s, nothing added), a run of 3 pages in
  13.5 s in a debug build ("Text found on all 3 pages", 96% sure, one
  undo step), Find found "condensation" on every page within 1.2 pt of
  where it was drawn on the tilted page, Show Recognized Text drew 108
  words, a word corrected and found, saved, reopened and found again.

Not done:

- Show Recognized Text on a file opened later reads the words back from
  the invisible layer, without confidence; their boxes come from the
  layer font's glyph boxes and sit a little off the scanned words.
- A run renders pages through the shared tile cache (24 MB), so a long run
  can push the screen's tiles out and they are drawn again.
- If pages are removed while a run reads, its text isn't added (it says
  so); pages edited in other ways meanwhile get the text read from what
  was drawn when it was read.
- A pdf made by New from Images has no file until Save As; Save on it
  asks where (it is the same as Save As).

Limits (from the engine): fast language data, so a page read in a
language it isn't in loses accuracy; right-to-left and vertical text are
written left to right; only the first page of a multi-page TIFF. See
core/ocr/README.md.

### Stage: conversion and export

Works:

- **Export To…** (document menu, File menu, Ctrl Shift E): one sheet with
  what this computer can make from the PDF, from core/convert's
  `capabilities()`: Word Document (Exact Match, the default, and Easy to
  Edit, side by side with the engine's sentence for each), OpenDocument
  Text, Excel Workbook, PowerPoint Slides, Images (PNG, JPEG with quality,
  TIFF in one file or per page; resolution; colour or gray), Web Page
  (exact or easy to edit; one file or with a folder), Plain Text (reading
  order or keep layout). Page ranges typed as "1-3, 7", checked with a
  sentence before the save panel opens. Progress with Cancel, and a result
  line from each engine's report ("2 slides. Text is real text; …").
  Unavailable choices stay in the list, greyed, with the reason.
- **Easy to Edit Word and OpenDocument** built in shell-core from the
  structure analysis: headings in the document's heading styles,
  paragraphs whose runs carry only what differs from the body text (the
  body font and size in Normal), bulleted and numbered lists with nesting,
  tables with column widths, spans, a repeated header row, shading and
  ruled edges, pictures at their size on the page, footnotes as real notes
  replacing the raised marker, the repeated header and footer as real ones
  with page number fields, a section per run of pages of one size with
  margins from the text. Opened in Word (private copy) without repair, with
  Heading 1/2 styles, a three-item bulleted list and one table; read by
  LibreOffice headless.
- **Exact Match Word** through core/exact's `convert`, with its cache in
  the app cache, verification when Word is installed, our renderer, progress
  and Cancel. The result shows the engine's check sentence, "How close is
  it?" with the measured numbers, and each font stand-in with its reason;
  without Word it says the result was not checked on this computer.
- **Office files to PDF**: spreadsheets and presentations (xlsx, xls, ods,
  pptx, ppt, odp) opened or dropped are shown as PDFs that Office or
  LibreOffice makes in the temp folder, with a line naming the program;
  `convert_office_to_pdf` makes one where the person chooses.
- **Open PDF as Document…** (writer and viewer document menus) opens a new
  window with the PDF rebuilt as an editable document in memory, named for
  a free ".docx" beside the PDF.

Not done, and limits:

- The writer opens a converted document as not edited, so its Save is
  greyed until something changes; Save As works. (The writer marks
  converted files with a notice; making them start unsaved is the writer's
  to decide.)
- Exact Match makes the whole document only; a page range is refused with
  a sentence.
- The PDF's internal links ("see page 4") stay plain text in Easy to Edit
  files: once text flows there is no page to point at.
- Easy to Edit keeps paragraphs that continue on the next page as two, as
  the analysis does.

### Stage: AI tools

Works:

- Off by default. With the switch off there is no AI Tools button; Find a
  Topic (Ctrl Shift F, or Topic… in Find) works on this computer with the
  engine's offline finder, and says so.
- Turned on in Settings: a side panel with Summarize (short, medium,
  detailed, page range), Key Points, Ask (answers stream; citations are
  chips that go to the page and outline the passage; "The document doesn’t
  say." as it is; quotes the engine couldn't find are flagged) and Find a
  Topic. A header always names the provider and whether text leaves.
- Before any request that sends text off the computer: the engine's
  disclosure sentence and preview, Cancel (focused) and Send, every time,
  unless "Don’t ask again for this provider and document" was ticked (one
  pair, while Letters runs; there is no global never-ask). A model on this
  computer says "This stays on this computer." and asks nothing. The shell
  enforces the same rule; the engine refuses without its acknowledgement.
- Stop; errors in the engine's words with Try Again or Open Settings as
  `is_temporary` says. Keys only in the system's credential store. Test
  Connection sends one fixed sentence. Start and stop a model server
  program the person chose, on 127.0.0.1.

Not done, and limits:

- Embedding settings (the engine supports them) aren't in Settings yet.
- The text for the tools comes from the structure analysis of the shown
  version; a scanned page without recognised text has none.
- The "don't ask again" memory ends when Letters quits, on purpose.

### Stage: packaging

Works:

- `tauri.conf.json`: file associations for pdf, docx, odt, ods, odp, rtf,
  png, jpg/jpeg, tif/tiff, bmp, gif and webp with names, roles, MIME types
  and macOS content types; per-user NSIS installer; resources (pdfium, the
  OCR engine, bundled fonts, document icons, `THIRD-PARTY-LICENSES.txt`);
  product metadata; macOS minimum version.
- `installer-hooks.nsh`: document icons and the Default Apps entry
  (RegisteredApplications and Capabilities) under HKEY_CURRENT_USER,
  removed on uninstall. The app itself never writes the registry; Settings
  opens Default Apps for the person and explains the two steps.
- Opening by type: several files on the command line open several windows
  (and through the single-instance path); a mac's open-document event too.
- `THIRD-PARTY-LICENSES.txt` from `cargo metadata` and the ui's lockfile
  (630 crates, 17 shipped npm packages): nothing outside MIT, Apache, BSD,
  ISC, Zlib, OFL, MPL and similar; libiconv (LGPL, with the OCR engine) and
  the Visual C++ runtime question are flagged for review.
- Settings window (Ctrl ,): General, Typing, Files, Fonts, Scanning,
  Accounts, AI Tools, Privacy; `docs/PRIVACY.md` lists every network call.

Not done, and limits:

- Nothing for macOS was built or run; the README lists what to run there
  and what to send back. Not signed or notarized on either system.
- The release build fails in core/exact with the repository's release
  profile on this GNU toolchain (LLVM "SmallVector unable to grow" while
  optimising letters-exact; with fat LTO it also ran out of memory). The
  installer was built with LTO off and letters-exact at opt-level 1
  (`npx tauri build -- --config "profile.release.package.letters-exact.opt-level=1"`
  with `CARGO_PROFILE_RELEASE_LTO=false`): 42.7 MB, per-user, not run here.
- Tauri's own association step sets each extension's per-user default
  class (`HKCU\Software\Classes\.pdf` = `Letters.pdf`, keeping the old
  value as `Letters.pdf_backup`). Windows still lets the person's own
  choice in Default Apps win, but this is more than "register and let the
  person choose"; an `OpenWithProgids`-only registration would need a
  custom NSIS template.
- Document icons on Windows show only for types Letters is the default
  for; macOS document icons aren't set (Tauri's config has no per-type icon).

### The font library

A large library of fonts, custom fonts and a font searcher. Proven in the
real app (Ctrl T opens it): a typo search, a language filter with sample
text, adding a WOFF2 file, downloading one family from the online library,
the Missing Fonts view and the font picker.

Works:

- One index of every font: the 411 faces installed on this Windows 11
  computer, fonts the person adds (.ttf, .otf, .ttc, .woff, .woff2 or a
  folder, checked and copied into Letters' folder, removable), 14 open
  families shipped with Letters (`tools/fetch-fonts`, about 18 MB, all SIL
  OFL 1.1) and the online library's 2,042 open families (catalogue shipped,
  257 kB, so search works offline): 2,274 families in all.
- The first scan runs in the background with progress (0.85 s release,
  1.2 s debug on 388 files); later starts read a cache (0.09 s).
- Search as you type, forgiving typos ("garmond", "atkinsn"), by kind,
  language, sample text ("draw: ĐỒNG ₫"), place, embeddable, favourites,
  and "similar to Calibri" (Carlito first, with the same widths).
- Each family's styles, languages, licence, embedding permission in a
  plain sentence and where it comes from.
- Downloads only when asked, after a one-time notice, from
  raw.githubusercontent.com at a pinned commit, with progress, cancel,
  resume and a hash check of every file.
- Missing Fonts: each font a document asks for that isn't here, the
  stand-in (metric-compatible where one exists) and Download or Replace.
- `FontPicker` for the writer and the PDF text tools: favourites, recent,
  document fonts, then everything, each name in its own face, virtual,
  keyboard friendly.

Not yet:

- The writer and the PDF text tools don't use the picker yet, and no
  screen calls `setDocumentFonts`, so Missing Fonts shows only when the
  library is opened with a document's fonts.
- The macOS menu has no Font Library item yet (Ctrl T works everywhere).
- Previews don't shape complex scripts; variable fonts embed their
  default instance; CFF fonts embed whole.
- Caladea doesn't have Cambria's widths in the library's version, so it is
  offered as similar, not metric-compatible.

### Stage: word-processor mode (the writer)

`.docx` and `.odt` files open as pages on the desk and edit like a word
processor. Proven in the real app with a document Word itself made
(headings, a numbered list, a table, a footnote, a picture, a comment and
a tracked change); the scripts and screenshots are listed in the stage
report.

Works:

- Opening by type: `.pdf` to the viewer, `.docx`/`.odt` to the writer,
  anything else a plain "can't open yet" (an old `.doc` says to save it as
  `.docx`). The Open… panel lists both; Recents shows "Word document" with
  a plain page picture.
- The model in the editor is lossless: model -> editor -> model gives back
  exactly what was read when nothing was edited, Word's paragraph ids,
  preserved content, list numbering, style ids and tracked changes
  included (tested on a Word-made file and its ODT copy). Content Letters
  can't edit shows as a quiet chip ("Chart — kept as it is"), saved back
  exactly, and can't be deleted by accident.
- Save writes the document's own format safely (temporary file, flush,
  rename); Save As chooses by name from Word, OpenDocument, PDF, Web Page
  and Plain Text, each with a sentence on what it keeps. Before .odt it
  says what ODT can't keep. "— Edited" in the title, the unsaved-changes
  sheet, undo and redo (the editor's history) work as on the PDF side.
- Recovery copies every 5 seconds while there are unsaved changes, offered
  the next time the file opens after a crash (proven by killing the app).
- Styles: the Style menu (Title, Heading 1-3, Body, Quote and the
  document's own), Ctrl Alt 1-3 and Ctrl Alt 0, using the document's real
  heading styles so Word sees them. The Outline sidebar updates live, jumps
  on click, promotes and demotes, and moves whole sections by dragging or
  Alt Up and Down.
- Smart indentation: Tab at a paragraph's start gives a half-inch
  first-line indent (then indents the paragraph), Shift Tab takes it back,
  in lists they change the level, Backspace removes a first-line indent
  first. The ruler has three labelled markers (First Line, Left, Right),
  draggable and keyboard-adjustable, values in inches or centimetres by
  region, tab stops by clicking, and Explain shows the engine's sentences.
  MLA and APA presets in the Document tab, one undo step.
- Typing: autocorrect, auto-capitalisation and smart quotes run the
  engine's rules (ported to TypeScript with the same test cases, so no
  keystroke waits for the shell); Backspace right after a correction puts
  back what was typed. Four switches (Autocorrect, Capitalize, Smart
  Quotes, Spelling) in the status bar and the Document tab, stored once per
  person. Spelling is the web view's own checker in the document's
  language: on Windows WebView2 uses the Windows spell checker and the
  languages installed there; on macOS WKWebView uses the system's.
- The Format inspector follows the selection: text, paragraph, lists,
  tables (rows, columns, merge and split, borders, shading, header row)
  and pictures (size keeping proportions, alt text, wrap). Insert: table
  grid, picture, page break, footnote, link, page number, comment. Review:
  Track Changes (typing and deleting are recorded), accept and reject one
  or all, comments. Find and Replace with Replace All as one undo step.
- Pictures: paste from the clipboard and drop from the system; eight
  handles resize with a transform preview and one undo step (Shift
  unlocks proportions, Alt from the centre, snapping to the original size
  and the text width, a live size readout); Original Size and Fit Width;
  Alt and the arrows resize, Alt Shift and the arrows move a free picture.
  In Line or Free in one click (Ctrl Alt Shift L or F): Free starts
  exactly where the picture was, and Word keeps it where it was dragged.
- Strict Word mode (status bar and Document tab): Word lays out the pages
  in the background after edits settle, and the editor breaks pages where
  Word does ("Layout checked with Word a moment ago"); without Word it says
  page breaks are estimated. A compatibility line says what would differ.
- Export as PDF and Print through Word (a private hidden instance, never
  the person's Word windows), else LibreOffice, else the web view's own
  printing with a note. Print also works for PDFs (pages drawn at 300 dpi,
  at their own paper size).
- Save to Google Drive: into Google Drive for desktop's folder when it is
  installed, or direct upload (as .docx, as a Google Doc, or as a PDF)
  through Google's sign-in in the browser with PKCE and a loopback
  redirect, `drive.file` scope only, the sign-in kept in the system's
  credential store, a confirmation that says exactly what is sent where.
- macOS Format and Insert menus (never run on a Mac).

Second round (the writer's objects, Strict Word mode, formats, print),
proven in the real app with real pointer and keyboard input; the scripts,
numbers and screenshots are in the stage report.

Works:

- Text blocks: Insert › Split into Text Blocks (Ctrl Alt Shift Enter)
  turns the paragraph at the cursor into two movable blocks; Make Text
  Block (Ctrl Alt Shift T) puts the selected paragraphs in one; Return to
  Text (Ctrl Alt Shift R) puts the text back at the nearest place, with a
  blue line showing where while you point at it. Each is one undo step,
  done by the office engine (`floating`). Typing, styles, lists and tables
  work inside a block; its edge drags it, its handles resize it with the
  text reflowing inside, and the keyboard moves (Alt Shift arrows) and
  resizes (Alt arrows) it. Word opens the saved file with the blocks as
  shapes at the places set, to the twip, with their text. Files with text
  boxes made by Word and by LibreOffice round-trip exactly.
- Pictures: In Line or Free through the engine (`float_image`,
  `inline_image`): Free starts where the picture was, measured within
  0.05 px at three zooms. Text flows around free pictures and text blocks
  on screen (square, tight and through drawn as square to the wider side;
  above and below; behind and in front), re-wrapping once on release.
  Double-click crops (the model's crop, which Word shows); the round
  handle turns in 15° steps (Shift frees it); Lock Proportions, Original
  Size, Fit Width, exact width, height, place and angle; Front and Back;
  Shift-click others to line them up or space them evenly; pink snap
  guides from the engine's snap rules (page edges, margins, centre, other
  objects). Copying a picture puts its pixels on the system clipboard;
  pasting pictures, picture files and html with pictures goes through the
  engine (bitmaps become png). Resizing and dragging in a 30-page
  document with 20 pictures never blocked a frame for more than 21 ms.
- Strict Word mode is the engine's: what is offered comes from
  `feature_matrix()`, the line and its Show list (with Go To and Fix, and
  Fix All) from `word_compatibility()` and `apply_fixes()`, and switching
  to Strict asks first to fix what isn't Word-exact. Word lays out the
  document through the engine's bridge, and the editor breaks its lines
  and pages exactly where Word does: every line of a 16-page essay starts
  with the same word as Word's, and the page counts agree. Word's check
  runs 4 seconds after typing pauses (or with Check Now), never while
  typing; a cold check took about 21 s, a cached one 18 ms, and the status
  bar says when the layout was checked and whether edits came after.
- Letters' own layout engine (`core/layout`) is the second provider: the
  Document tab's "Letters’ Own Layout" uses it for Free mode, applied the
  same way as Word's. On the 16-page test essay it broke all 580 lines and
  all 16 pages exactly where Word did, in 0.23 s.
- Nothing moves when you click: a real-app check compares every block's
  and object's box after clicking into text, a picture, a table, crop mode,
  selecting and toggling panels; opening and saving without edits keeps
  the document as the engine read it (text blocks and kept content too),
  and object kinds the engine is adding (shapes, groups) pass through the
  editor untouched as kept chips.
- Pages in Free mode split a paragraph between lines when it doesn't fit,
  with widow and orphan control, keep with next and keep lines together;
  footnotes sit at the bottom of the page their number is on.
- Save As lists the engine's formats with its sentence on each: Word,
  OpenDocument, PDF, Rich Text, Web Page, Markdown (pictures beside it),
  Plain Text and, with Word or LibreOffice, Word 97–2003. `.rtf` and `.doc`
  files open in the writer by conversion and save as a `.docx` beside
  the original. Word opens the RTF Letters writes without complaint.
- Print opens Windows' own print dialog for Word's print-ready pdf (the
  bridge's, or LibreOffice's with a note that breaks can differ), at the
  pdf's page size, one sheet per page.
- Tracked changes record formatting (Word's `w:rPrChange`), paragraph
  splits and joins (the paragraph mark's revision); accept and reject
  handle them, and hovering shows who and when.
- The Format inspector's font control is the font library's picker; fonts
  the document uses that aren't installed load from the library; the
  status bar's missing-font count opens its Missing Fonts view, whose
  Replace changes the font everywhere in one step.
- The Drive sheet sends the client ID to Settings › Accounts.
- Word is only reached through the engine's bridge (a private hidden Word
  found by its window caption, with a watchdog); the writer's own Word
  scripts are gone, and the checks left no Word or LibreOffice running.

Partial or not done:

- Wrapping draws square, tight and through the same way, on the wider
  side of the object; Word can fill both sides and hug a picture's
  outline. Two objects whose wraps overlap on the same side can sit side
  by side on screen.
- A page that Word starts inside a table starts at the table on screen.
  Justified paragraphs in Strict mode keep Word's line breaks but their
  last-before-break lines are not stretched.
- Free-mode pages are still measured on screen; only Strict mode (Word)
  or the engine setting give exact pages. Columns show as one column.
- Footnotes are edited where they are shown, at the bottom of their page;
  endnotes stay under the text.
- Direct Drive upload is still tested against a local stand-in for
  Google's servers only: there is no client ID to sign in with for real.
- The Windows print dialog is shown and proven up to choosing the printer;
  see the stage report for the run that printed to Microsoft Print to PDF.
- "Open PDF as Document…" belongs to the conversion stage. The menu bar's
  Command Option 1-3 stay with the View menu on a Mac.

### Stage: forms and signatures

Everything here happens on this computer: no server, no time stamps
fetched, and nothing records who opened, filled or signed a form.

Works:

- **A Fill & Sign tab** in the Tools inspector: Fill Form, Find Fields…,
  Add Field, Edit Fields, Flatten Form…, Sign with a Picture, Sign with a
  Certificate and Signatures, each a disclosure section with its
  explanation, its few settings, one button and a line saying what will
  happen. Explain lists them (and Show Fields, Next Empty Field and Create
  a Self-Signed Certificate…) first while the tab is open.
- **Filling a form on the page.** Fields show as soft tinted boxes (Show
  Fields turns the tint off). Click or Tab into a field and type right
  there: text, several lines, combs (one letter per box), dates (in the
  form's own pattern, or 2026-11-11, with a calendar button), checkboxes
  and radio buttons (click or Space; arrows move within a group), pop-up
  menus and lists (the system's own controls; an editable menu takes
  typed answers). Tab and Shift Tab go through the fields page by page in
  rows, scrolling each into view, also on turned pages. Typing is stored
  when the field is left, so each answer is one undo step. Required
  fields carry a red asterisk; the engine's refusals show beside the field
  in plain words ("This field is required. Type an answer.").
- **The field list** (name or help text, page, answer or Empty, required),
  a progress line ("7 of 12 filled") and **Next Empty Field** (Ctrl Alt E,
  Command Option E on a mac).
- **Honest messages**: an XFA form shows the engine's message in a banner
  and the tab; a form with calculation scripts says "This form has
  automatic calculations that Letters doesn’t run. Totals may need to be
  typed in."
- **Find Fields…** on a flat form (this page or all pages): suggestions
  appear as dashed outlines with their labels and kinds; accept all, or
  accept, reject, move and resize each on the page (the shared frame) and
  change its kind in the list; Add Fields makes the accepted ones real
  fields in one undo step. It says it is a suggestion and that scanned
  pages need text recognition (a later stage).
- **Add Field**: text, text with several lines, checkbox, radio buttons,
  pop-up menu, date or signature; drag on the page, click for the usual
  size, or press the button from the keyboard to put one in the middle of
  the view. **Edit Fields** is a mode apart from filling: select a field,
  move and resize it (drag, arrows, Alt with the arrows), delete it, and
  change its name, help text, required, alignment, text size and choices.
- **Flatten Form…** with the meaning in one sentence, the warning that
  answers can't be changed afterwards, and signature fields kept unless
  unticked.
- **Sign with a Picture**: draw (pointer or pen, smoothed, Clear), type
  (four handwriting-like styles from fonts already on the computer) or
  choose a PNG or JPEG (with Make White See-Through). Kept signatures stay
  in the app's data folder, never in a document, and can be deleted.
  Place one with a click (1.75 inches wide), a drag, a click inside an
  empty signature field (fitted in proportion), or from the keyboard. It
  is a picture annotation, so Select moves and resizes it in proportion.
- **Sign with a Certificate**: choose a .p12 or .pfx and type its password
  (used once, never kept), or **Create a Self-Signed Certificate…** (name,
  email, password; it says plainly what self-signed means). The signature
  goes into an empty signature field, a place drawn on the page, or
  nowhere visible, with an optional picture, reason and place. Unsaved
  changes are saved first ("Save and Sign"); the saved file's bytes are
  signed and written back exactly, as an incremental update.
- **Signed documents are guarded.** The first change asks "Changing this
  document will make its signature show as changed. Save a copy instead?"
  (Save a Copy…, Cancel, Edit Anyway). After Edit Anyway, Save is refused
  with a message pointing to Save As, so the signed file is never
  rewritten; Save a Copy puts the change in a new file and leaves the
  signed one as it was.
- **Signatures**: who signed, when, Valid / Changed after signing /
  Damaged, the reason and place, self-signed noted, and the engine's note
  that Letters doesn't check trust lists. A quiet banner at the bottom of
  the window says who signed and whether it has changed since.
- macOS: a Forms menu with every command.

Proved in the real app (debug build, real pointer and keys over the
WebView2 debugging port): every field kind filled from the keyboard only,
on an upright and a turned page, saved, reopened, and the text drawn;
the refusal beside a required field; Find Fields, Accept All, Add Fields
and filling a found field; a field drawn, renamed, moved, deleted and
brought back with Undo; Flatten and Undo; drawn, typed and chosen
signature pictures kept, placed and deleted, one into a signature field;
a self-signed certificate made, the form signed and checked as valid,
`openssl cms -verify` accepting it; Edit Anyway reported as Changed
after signing with Save refused and the file untouched; Save a Copy;
a second signature in a drawn place; the XFA message; a narrow window.

Not done:

- Remembering a certificate password: not offered.
- Line breaks typed in a field with several lines are sent as spaces,
  because the engine refuses them (the words still wrap to the box).
- A form's own tab order (`/Tabs`, the widget order) isn't read; Tab
  goes in rows, the order Letters gives new fields.
- Radio buttons can't be added to or removed from an existing group, and a
  field's kind can't be changed (the engine's `update_field` can't).
- Find Fields: on the test form, the line after "Parent or guardian:"
  wasn't proposed, and separate small boxes with gaps between them came
  out as checkboxes rather than one comb. Scanned pages give nothing.
- A signature picture is a picture annotation, not a part of the page,
  until comments are flattened.
- macOS: the Forms menu and Command Option E have not been run on a Mac.
- Screen readers: the live announcements were read back from the page,
  not heard through Narrator or VoiceOver.

Limits (from the engine): no JavaScript is ever run; text is limited to
Western European letters; no time stamps, revocation or trust-list checks;
signing uses SHA-256 and RSA or P-256 keys.

Signing and Protect: a document with a password to open or restrictions
(set under Protect, or still on the file) can't be signed yet. The engine
signs only unencrypted files, and locking a signed file afterwards
rewrites it and breaks the signature, so Letters refuses with "Letters
can’t sign a document that has a password or restrictions yet. Remove
them under Protect in Tools, save, then sign…" instead of quietly
dropping the password; Save is also refused for a signed file whose
password changed. After a redaction, the clean file on disk is what gets
signed (never the working copy, which may still hold unused objects),
the versions kept for undo are deleted as a clean save deletes them, and
Save As of the unchanged signed file copies it exactly.

### Stage: redaction and security

Works:

- **A Protect tab** in the Tools inspector: Mark Text, Mark Area, Find and
  Mark…, Review Redactions…, Apply Redactions…, Remove Hidden
  Information…, Unlock for Editing (only for a locked PDF), Require a
  Password to Open, Restrict What Others Can Do and Remove Password, each
  with its explanation, its "what will happen" line and an Explain entry.
  A macOS Protect menu has the same commands.
- **Marking first, applying later.** Marks are red, dashed, hatched boxes
  drawn over the page, never written to the file: by text selection (one
  area per line), by dragging a box, or by Find and Mark… (a phrase, or one
  click for email addresses, phone numbers or long numbers, listed with
  page, the words around each match on its line, and a tick box each;
  Mark All). Marks are listed, shown, removed, moved and resized with the
  shared frame. A page changed by another tool (turned, cropped, moved,
  deleted) loses its marks with a notice. Closing the document with marks
  not applied asks first.
- **The review** shows, per mark, the exact text that goes, pictures,
  drawings, comments and form answers, pictures removed whole or partly
  blanked, the engine's warnings (with Find and Mark… for words that also
  appear elsewhere, such as in the title), that fonts stay, and the page
  now and after, drawn from a redacted copy (a dry run in the session
  folder; the working copy is untouched).
- **Apply Redactions…** asks first, in a sheet that counts the areas, says
  it is permanent once saved and lists what is not covered, with Black or
  White boxes and an optional word in them. It is one undo step.
- **Clean saves that can't be bypassed.** After a redaction or a scrub the
  session saves through the engine's `save_clean` (the whole file written
  from scratch, nothing unused or old kept), for Save and Save As, and
  there is no way back to plain copying. The stored working copy is
  rewritten clean too. Saving deletes the undo and redo versions, which
  still hold the removed content; closing deletes the session folder; a
  crash leaves it until the next start, which deletes it.
- **Remove Hidden Information…** lists what the file has (author and
  properties, metadata, attachments, scripts, hidden layers, comments, form
  answers, thumbnails, app data, private data, unused content, links,
  identifiers) with counts and details; comments and form answers start
  unticked.
- **Passwords**: an open password (AES-256, typed twice, Show password, a
  strength hint), restrictions (printing none, low quality or full;
  copying, changing, comments, forms, pages; an owner password), Remove
  Password, each applied when the file is saved, with a lock line saying
  so. Setting one marks the document changed so Save is offered.
- **Unlock for Editing**: a locked PDF opened with its password can now be
  changed. The password unlocks it (the open or owner password; a file
  whose author forbade changes needs the owner password), the decrypted
  working copy lives only in the session folder, and saving locks it
  again the same way (AES-256, with its permissions) unless changed. A
  file unlocked with its owner password won't save until the person
  chooses whether it keeps a password to open. An edit tried before
  unlocking says where to unlock.
- **No picture of a password protected file** is kept for Recents.
- **Keyboard**: Ctrl Alt R (Control Command R on a mac) marks the selected
  text or picks Mark Text; Ctrl Alt B picks Mark Area and, pressed again,
  places a box in the middle of the page with the focus, ready for the
  arrows (Alt to resize, Delete to remove). Screen readers hear "3 areas
  marked on page 2."
- **Tests**: 27 unit tests for the pure modules (marks, review, find and
  mark, password strength, permissions, hidden information, the
  sentences); 18 shell-core tests (`protect/`): clean saves after
  redaction and scrubbing for Save and Save As, no way to bypass them,
  undo versions holding the secret deleted on save, nothing left after
  close or after a crash and restart, unlock, edit and save locked again,
  the owner password rule, merging password changes. A script drove the
  real app with the mouse and keyboard (50 checks): a PDF with a secret in
  its text, an email address twice, a picture, a comment and its title and
  metadata was marked by selection, by area and by the email pattern,
  reviewed, redacted, scrubbed and saved; outside the app the saved file's
  bytes, inflated streams and objects (qpdf) and its text (pdfium) held no
  trace of any secret, and the app's cache held no copy after closing.
  Then an open password was set and saved (qpdf: AES-256, revision 6,
  refused without it or with a wrong one), the app was killed and
  restarted (the plain working copy left by the crash was deleted), the
  password prompt refused a wrong password and opened with the right one,
  and the file was unlocked, turned and saved locked again.

Not done:

- Redacting a page whose content can't be read is refused by the engine;
  the app shows its message but offers nothing else.
- The before and after pictures show one page at a time at a small size.
- Marks are not kept when the document is closed (on purpose) or moved
  with their page when another tool reorders pages: a page that looks
  different loses its marks, but two pages that look alike (same size, no
  text, such as two scans) can't be told apart.
- Signing (Fill & Sign) writes exact bytes and does not go through the
  clean save writer, so signing a document that has a password set in
  this session would save it without the password. Not changed here
  (it is the Fill & Sign code); see the report.

Limits (from the engine, said in the app where it matters):

- Fonts stay in the file and can show which letters were used. Glyphs
  next to a box whose ink reaches under it are kept. Images the engine
  can't decode (scans in JBIG2 or CCITT) are removed whole when touched.
  The same words elsewhere (other pages, title, bookmarks, comments) are
  reported, not removed. See core/redact/README.md, "Known limits".
- Restrictions are a request other apps may ignore; only the open password
  encrypts. Re-locking an unlocked file always uses AES-256, and its owner
  password is replaced by one nobody knows unless the person unlocked it
  with the owner password or sets one.
- While a document is open its working copy (decrypted, for an unlocked
  one) sits in the app's cache folder; it is deleted on close, and after a
  crash at the next start of Letters.

### Stage 3: annotations

Works:

- **An Annotate tab** in the Tools inspector, beside Pages: Selection,
  Highlight, Underline, Strike Through, Note, Text Box, Shapes, Draw,
  Picture, Comments, Flatten Comments… and Name on Comments, each a
  disclosure section with its explanation, few settings, one button and a
  line saying what will happen. Explain lists them first while the tab is
  open.
- **Selecting text on the page.** Drag across text (it snaps to
  characters), double-click for a word, triple-click for a line, Shift-click
  to extend; it runs across lines and pages and is shown in the selection
  colour. Shift with the arrows extends it from the keyboard (Ctrl or Alt
  for whole words), and Ctrl C copies it. Works on turned pages and in two
  page mode. The engine now says where every character of a page is
  (`page_text`).
- **The selection bar**: a small floating bar beside selected text with
  Highlight, Underline, Strike Through, Note and Copy.
- **Highlight, Underline, Strike Through** in both orders: select and then
  press the tool, or press the tool and then drag across text. A selection
  across pages is one undo step. Light highlight colours, strong ink
  colours; the accent blue is never offered.
- **Notes**: click to place, type in a pop-up beside it; replies, editing
  and deleting there or in the **Comments** list (author, time, page; a
  click goes to the note). Deleting a note deletes its replies.
- **Text boxes** typed straight onto the page in a field that looks like
  the result (font, size, bold, italic, colour, alignment, background);
  double-click one to change its words.
- **Shapes**: rectangle, ellipse, line and arrow, with colour, width, fill
  and opacity; Shift for a square, a circle or a 45 degree line.
- **Draw**: freehand ink with colour and width, and a wide see-through
  Highlighter. Strokes made without a one second pause become one drawing,
  so Undo takes back the whole gesture.
- **Picture**: choose a PNG or JPEG, click to place it two inches wide or
  drag to size it; it keeps its proportions, also when resized.
- **Selecting, moving, resizing, restyling and deleting** any annotation
  (other apps' too, where the engine says Letters can change them): a
  frame with eight handles (four for pictures), arrows to nudge (1 point,
  10 with Shift, Alt to resize), Delete, Escape. The Selection section
  shows only the controls that fit the kind, and changing one restyles it.
- **Flatten Comments…** with the engine's own explanation and "They can’t
  be edited afterwards, but Undo still works until you close the document."
- Every action is one undo step and marks the document edited; holding an
  arrow key makes one step when it is let go. Annotations are drawn by the
  engine through the normal tile refresh; while dragging or drawing a
  preview is drawn at once.
- **Keyboard**: Ctrl Alt H, U, S (Highlight, Underline, Strike Through),
  Ctrl Alt N (a note in the middle of the view, ready to type), Ctrl Alt T
  (a text box the same way), Ctrl Alt D (Draw), Escape (back to Select).
  On a mac the same letters with Control and Command, as in Preview. A
  section's button pressed from the keyboard places its note, text box,
  shape or picture in the middle of the view with the focus on it.
  Screen readers hear "Highlight added to page 3."
- macOS: an Annotate menu with every command.
- **The page tool layer**, made to be reused by redaction and forms: pointer
  to page points at any zoom, mode and scroll; text selection; rectangle,
  line, point and ink tools; the frame with handles; one active tool at a
  time. See "Working on the page: page tools" in ARCHITECTURE.md. The crop
  rectangle now uses the same frame.
- The author name on new comments is a setting (Name on Comments), starting
  as the computer's user name.
- Fixed: the message when a watermark picture can't be read showed garbled
  quote marks. Fixed: the PDF engine's tests could crash now and then when a
  worker stopped while another was drawing.

Measured on Windows 11, debug build: a highlight took 140 ms from the click
to the new state on a 4 page file, and 184 ms on a 500 page file (page 250).
Listing every annotation of the 500 page file took 200 ms, undo 194 ms.
Scrolling through 100 pages with the annotation layer and page text being
fetched had no frame longer than 10 ms. Only 3 pages were in the window at
a time.

Not verified or not done:

- macOS: the Annotate menu and the Control Command keys have not been run
  on a Mac.
- A pen's pressure is ignored; strokes have one width.
- While an existing text box is being edited, its old words are hidden by
  a nearly white field rather than removed from the tile, so on a coloured
  page the field shows as a light box until the edit is done.
- Highlighting text in a scanned page needs OCR first (a later stage):
  without text, dragging selects nothing.
- Drawing freehand needs a mouse, pen or finger; every other tool works from
  the keyboard.
- Selected text is not auto-scrolled when a drag reaches the window's edge.

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

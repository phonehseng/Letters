# Changelog

All notable changes to Letters are written here, newest first.

## Unreleased

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

Partial or not done:

- Pagination in Free mode is an estimate (measured blocks; a block taller
  than a page is not split); exact pages come from Word in Strict mode or
  on export. Footnotes show together under the text, not per page.
- Floating pictures are drawn where they are but text does not wrap
  around them on screen; crop and rotation handles are not done; copying a
  picture to other apps carries its address only once it was shown.
- Free text blocks (Split into Text Blocks, Make Text Block, Return to
  Text) are not done: they need the office engine's text-box model, which
  is being added.
- Strict mode uses Letters' own feature list until the engine's
  `feature_matrix()` and `word_compatibility()` exist; Word's layout is
  matched to blocks by paragraph order and falls back to the estimate when
  a block Letters keeps opaquely holds paragraphs of its own.
- Rich Text and Markdown in Save As are shown as coming (engine
  exporters). Columns show as one column on screen.
- Direct Drive upload is tested against a local stand-in for Google's
  servers only: there is no client ID to sign in with for real.
- Fonts: missing fonts are counted in the status bar and fall back to an
  open font with the same widths where one is known (Carlito for Calibri,
  Tinos for Times New Roman...) if it is installed; the font library's
  picker, served fonts and Missing Fonts view are not wired yet.
- "Open PDF as Document…" is in the menu, greyed: "Coming with PDF
  conversion."
- Formatting changes and paragraph splits are not recorded as tracked
  changes. The menu bar's Command Option 1-3 stay with the View menu on a
  Mac, so headings use the Style menu there.

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

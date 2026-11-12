# Design

Letters is designed the way Apple would design a word processor: the
document is the star and the app steps back. This file records what that
means in practice, where each rule comes from, and how it is applied here.
Read it before changing anything visual.

The rules come from Apple's Human Interface Guidelines (HIG) and its design
talks, read in November 2026. Links are at the bottom.

## What makes Apple's interfaces different

It is less a look than a set of habits. Most of them are about leaving
things out.

1. **Content and controls live on two layers.** The document is the content
   layer. Toolbars, sidebars and inspectors are a separate functional layer
   that floats above it. Content runs to the window's edges and scrolls
   underneath the controls instead of being boxed in by them. The floating
   layer uses a translucent material so you keep a sense of what is behind it.
2. **Hierarchy comes from layout, not decoration.** Importance is shown by
   position, grouping, spacing and type weight. Colour is the last tool,
   not the first. Emphasis that everything has is emphasis nothing has.
3. **One accent colour, used rarely.** Toolbar symbols are monochrome. The
   accent is saved for the one action a screen most wants you to take, for
   selection, and for on/off state. A view gets one or two prominent
   buttons at most.
4. **Show less, reveal on demand.** Apple calls this progressive disclosure.
   Pages has no ribbon: a short toolbar holds the common actions, and
   everything else lives in an inspector on the right whose contents change
   with what is selected. Word shows every option all the time; Pages shows
   the options that apply right now.
5. **Things are where people expect.** A toolbar has three areas with fixed
   jobs: leading (back, sidebar toggle, title), centre (common controls),
   trailing (inspector buttons, search, the primary action). Groups are
   formed by function, and there are about three of them at most.
6. **One type family with a strict scale.** The system font, a small set of
   sizes, no light weights, and letter spacing that changes with size so
   small text is airy and large text is tight.
7. **Shapes nest.** Corners are concentric: an inner corner's radius is the
   outer radius minus the padding between them, so nested shapes look like
   they belong together. Small, dense controls are rounded rectangles;
   large or stand-alone ones are capsules.
8. **Words are plain and short.** Buttons are verbs in title case ("Zoom
   In"). An ellipsis means "this opens something else" ("Open…"). No "we",
   little "your", no "oops". An error says what to do next. An empty
   screen offers the next step.
9. **Motion has a job.** Animation is brief, explains a change, never runs
   on things you do constantly, and can be turned off.
10. **The app fits its platform.** Apple's word is harmony: match the system
    you run on. Use the system font and the person's settings (dark mode,
    reduced motion, reduced transparency, increased contrast). On a desktop,
    every toolbar action also has a menu command and a keyboard shortcut.

One more thing worth knowing: Apple itself pulled back. The 2025 "Liquid
Glass" look was criticised as hard to read, and the 2026 release made the
glass less transparent by default and set toolbar buttons more clearly apart
from the background. So legibility wins over glassiness here too.

## How Letters applies them

| Rule | In Letters |
|---|---|
| Two layers | The page scrolls under a floating toolbar. Only the toolbar groups and the side panel use the glass material; nothing in the page area does. A soft fade at the top keeps the toolbar readable (Apple's "scroll edge effect"). |
| Hierarchy from layout | The document name sits at the leading edge in bold, with the page number under it in a quieter colour. No boxes or divider lines around toolbar areas: spacing separates them. |
| One accent | Toolbar symbols are monochrome. The accent appears on the "Open a PDF" tile, on pressed toggles (Night Mode, Tools, Explain), on selected thumbnails and the drop line when moving pages, on selected text and the frame around a selected comment, on the one open tool's action button, and on focus rings. It is never a mark's colour. |
| Show less | The toolbar holds about ten actions in three groups. Night Mode and Two Pages sit in one View pull-down, as in Preview and Pages. The sidebar, the Tools inspector and the Explain panel are closed by default. Editing tools live in the inspector, not in a ribbon (see "The Tools inspector" below). |
| Expected places | Leading: the sidebar button, back to Recents, then the title, which is also the document menu. Centre: zoom. Trailing: Find, View, New Window, then Tools and Explain (both open side panels, and inspector buttons go on the trailing edge). On a narrow window items leave in a fixed order, least important first (`app/ui/src/lib/toolbar.ts`). |
| Type scale | The macOS text styles, in rem so they scale with the system text size: large title 26/32, title 17/22, headline 13 bold, body 13/16, subheadline 11/14, footnote 10/13. Nothing is smaller than 10. Letter spacing follows Apple's tracking table. The body style is set on `body`, never on `:root`: on `:root` it would make 1rem 13px and shrink the whole scale (that happened until stage 2, putting the footnote at 8px). |
| Nested shapes | Toolbar groups have a 14px radius and 4px padding, so buttons inside have a 10px radius. The same sum is used wherever shapes nest. |
| Words | All labels and tooltips live in `app/ui/src/lib/tools.ts`. Title case verbs, "Open…" with an ellipsis, errors that say what to do. |
| Motion | 150 to 200 ms, only on hover and panel changes, off when the system asks for reduced motion. |
| Fits its platform | System font on each OS (San Francisco on macOS, Segoe UI Variable on Windows). Follows dark mode, reduced motion, reduced transparency (glass becomes solid) and increased contrast (Apple's higher-contrast blue). Every toolbar action has a keyboard shortcut. |

### The Tools inspector

Pages has no ribbon: a short toolbar and an inspector on the trailing side
whose contents fit the task. Letters does the same.

- **One button, one panel.** "Tools" on the trailing edge of the toolbar
  (Ctrl Alt I, the Show Inspector key in Pages) opens the inspector. A
  segmented control at its top picks a tab: Pages and Annotate today;
  later stages add Fill & Sign, Protect and Accessibility. The control
  showed even with one tab, so the place where tabs appear was already
  familiar. Adding a tab is one entry in `app/ui/src/lib/inspector.ts` and
  one component.
- **Progressive disclosure.** Each tool is a disclosure section. Closed, it
  shows its name and one line about it. Open, it shows a sentence explaining
  the word if a first-year may not know it ("Cropping hides the outside of a
  page; it doesn't remove it…"), its few settings, one clearly named button,
  and under the button a live line saying exactly what will happen ("Adds
  “Page 1 of 30” at the bottom centre of 30 pages. Undo with Ctrl Z."). Only
  one section is open at a time, so the inspector never has more than one
  prominent (accent) button.
- **Sensible defaults.** The common case is one click: page numbers at the
  bottom centre of every page, a header with the file name and the date, a
  "Draft" watermark at 30%, a crop half an inch in from each edge.
- **What tools act on.** Pages selected in the thumbnails, or the page being
  read when none are; every tool says which ("Works on the page you are
  reading, page 3. To choose other pages, select them in the sidebar.").
- **Choices are segmented controls**, raised rather than coloured, like the
  sidebar's Thumbnails and Outline control.

### Working on the page: page tools

Annotating happens on the page itself, so the page needs a clear idea of
what a press or a drag means right now. Letters follows Preview and Pages:
there is always exactly one **page tool**, and the default is **Select**.

- **Select** selects text by dragging (double-click for a word,
  triple-click for a line, Shift-click to extend, Shift with the arrows
  from the keyboard) and picks up comments and marks with a click. A
  picked one gets a frame in the accent colour with eight square handles
  (four for pictures, which keep their proportions; none for marks on
  text, which belong to their words). It moves when dragged; the arrows
  nudge it a point at a time, ten with Shift.
- **Picking a tool** is a button in its section of the Annotate tab
  ("Highlight", "Add Note", "Draw Rectangle", "Start Drawing"). While a tool
  is in use its button is pressed and says how to stop ("Stop Drawing"),
  the cursor over the pages changes, and the line under the button says
  what to do on the page and that Escape stops. Escape, or closing the
  section, brings Select back. Notes and text boxes are placed one at a
  time and hand back to Select straight away, so the next click reads the
  note instead of making another.
- **Both orders work** for marking text: select first and then press
  Highlight, or press Highlight first and then drag across the words.
- **What is drawn is the result.** While a shape, a line or a stroke is
  being drawn, a preview is drawn over the page at once; when the press
  ends the real annotation replaces it. A text box is typed into a field
  that sits exactly where the box will be, in its font, size, colour and
  alignment, so there is no separate dialog.
- **The keyboard can do it all.** A section's button pressed from the
  keyboard puts a note, text box, shape or picture in the middle of the
  page on screen and gives it the focus, so the arrows can move it.
  Drawing freehand is the one tool that needs a pointer, and its line says so.
- **Words for screen readers.** Each frame's label says what it is and how
  to move, resize and delete it. After an edit a live region says what
  happened ("Highlight added to page 3.").

### The selection bar

When a drag across text ends, a small glass bar appears just above the
selection (below it near the top of the window): **Highlight, Underline,
Strike Through, Note, Copy**. It is the one place where marking text needs
no trip to the inspector, the way the floating bar over selected text works
in iOS and in Preview's markup. It uses the toolbar's material and button
shapes, carries text labels like the toolbar, never uses the accent, and
goes away with the selection or while a drag is under way. It is a
toolbar for assistive technology: Tab reaches it, the arrows move along it.

### Colours of marks

Highlight colours are light (yellow, green, blue, pink, orange), because
they are multiplied onto the page and the words under them must stay
readable. Ink, shapes and text use a few strong colours that print well.
The accent blue is never offered: it means "selected" on screen.

### Side panels: sidebar, Tools and Explain

The sidebar is on the leading side; Tools and Explain share the trailing
side, Tools nearer the document. The rule (`app/ui/src/lib/sidePanels.ts`):
the document always keeps at least 26rem. Opening a panel that doesn't fit
closes the panel opened longest ago. On a wide window all three can be open;
on a medium one two; on a small one they take turns.

Explain is context-aware: with the inspector open it lists the open tab's
tools first, under "Pages tools" or "Annotate tools", then the rest of the
screen (with text selected, the selection bar's buttons too). Words a
first-year may not know (extract, split, merge, crop, header, watermark,
strike through, note, text box, freehand ink, flatten) are explained there
and in the open section.

Below 46rem the inspector stops narrowing the document and floats over it
like a sheet, full height, with its own close button (Escape closes it too).
At the 480px minimum window nothing overflows: the toolbar drops its
least important parts as before, and the floating inspector covers the page.
A page tool picked there keeps working once the inspector is closed, and the
selection bar wraps onto two rows rather than leave the window; Escape
brings Select back.

### The writer (word documents)

A `.docx` or `.odt` opens as pages on the same grey desk as a PDF, as if
Pages opened a Word file: the text is the star, the controls float.

- **Toolbar.** Leading: Outline (the sidebar), back to Recents, and the
  document's name, which is the document menu (Save, Save As…, Export as
  PDF…, Undo, Redo, and "Open PDF as Document…" greyed until PDF
  conversion exists). Centre: the Style menu, the one formatting control
  in the toolbar because headings are what students need most (Title,
  Heading 1–3, Body, Quote, then the document's own styles), and zoom.
  Trailing: Insert (a table grid, Picture…, Page Break, Footnote, Link…,
  Page Number, Comment), Find, View (Ruler, Night Mode), New Window, Format
  and Explain. No B, I and U in the toolbar: they are a key away and in the
  inspector, which keeps the toolbar short.
- **Inspector**, on the trailing side like Pages, with three tabs.
  *Format* follows the selection: Text (style, font, size, bold, italic,
  underline, strikethrough, super and subscript, colour, highlight) and
  Paragraph (alignment, line spacing, spacing before and after, indents,
  lists and their level) for text; Table (rows, columns, merge and split,
  borders, shading, header row) when the cursor is in a table; Picture
  (size keeping proportions, alt text with one sentence on why it matters,
  wrap) when a picture is selected. *Document*: page size, orientation,
  margins, columns, header and footer, the MLA and APA presets (each with
  one sentence on what it sets, and that it changes the whole document;
  Undo takes it back), and Typing. *Review*: Track Changes, every change
  with Accept and Reject, and the comments.
- **Ruler.** Above the page, under the toolbar, aligned with the page.
  Margins are shaded; numbers count from the left margin in inches or
  centimetres by the system region. Three labelled markers: First Line
  (above the strip), Left and Right (below it). Each is a slider for the
  keyboard (arrows move an eighth of an inch or a quarter centimetre, Shift
  a larger step) with a tooltip in plain words ("the first line is
  indented 0.5 in"). Clicking the strip adds a tab stop; dragging one off
  removes it. Explain shows the engine's sentences for the paragraph.
- **Status bar.** A glass strip along the bottom: word count (of the
  selection when text is selected), "Page 2 of about 5" (an estimate, and
  it says so), the language, a "Tracking Changes" badge when on, and four
  quiet switches: Autocorrect, Capitalize, Smart Quotes, Spelling. A
  switch that is off is struck through. The same four switches are the
  Typing section of the Document tab, and both change one setting stored
  for the person.
- **Kept content.** Things Letters can't edit (a chart, a shape, an
  equation) show as a quiet dashed chip, "Chart — kept as it is", saved
  back exactly. Deleting a range around one is refused with a sentence;
  Backspace next to one selects it first, so it is only ever deleted on
  purpose.
- **Pages** are an estimate of Word's in Free mode: measured on screen,
  line by line, with a page gap where the next line no longer fits and
  Word's rules (two lines at least on each side, headings kept with the
  next paragraph), the next page's header drawn in the gap, and each
  footnote at the bottom of the page its number is on. The Explain entry
  and the status bar say so; in Strict Word mode the lines and pages are
  Word's own.
- **The page stays white** in dark mode, as paper does in Pages; Night Mode
  in View darkens it.
- **A selected picture** gets a thin accent frame with eight round handles,
  one round handle above it for turning (15° steps; Shift for any angle)
  and, above that, one small glass bar: In Line | Free (a segmented
  control, the one thing most people want to change), the wrap when Free,
  Front and Back, Crop, Lock Proportions, Original Size, Fit Width, exact
  numbers for size, place and angle, and one sentence saying what the
  current state means. While a handle is dragged only the picture changes
  on screen, with its size in a small label under it; nothing else moves
  until release, when the text re-wraps once. Pink guide lines show what
  it lines up with (page edges, margins, the centre, other objects).
  Double-click crops: the handles turn into square bars that cut a side
  instead of scaling; Return or Done ends it.
- **Text blocks** (Insert › Split into Text Blocks, Make Text Block, Return
  to Text) are text in a thin frame that sits anywhere on the page, with
  text flowing around it. A click inside types, as in any paragraph; the
  frame's edge (the move cursor shows) drags it; its handles resize it and
  the text reflows inside as you drag. Pointing at Return to Text draws a
  blue line where the text will go back. Shift-click a second object to
  line them up (Line Up in the bar).
- **Free and Strict Word** is one quiet pill in the status bar (accent when
  Strict), mirrored at the top of the Document tab. Next to it the status
  bar names where page breaks come from ("Pages estimated", "Layout checked
  with Word 2 minutes ago, before your last edits") with a quiet Check Now,
  and in Strict mode one line says whether everything will look the same
  in Word, with Show: a sheet with each difference in the engine's own
  sentence, Go To, and a Fix where Letters has one (Fix All at the bottom,
  the one accent button). Switching to Strict asks first, in the notice
  bar, whether to fix what isn't Word-exact. Word takes about half a minute
  for its first look at a document, so it never runs while you type: four
  seconds after you pause, or at once with Check Now.
- **Save to Google Drive…** is in the document menu and opens one sheet with
  the two ways, each in a sentence: the Drive folder on this computer, or
  direct upload after signing in. The upload button sits under a sentence
  that says exactly what leaves the computer and where it goes.

### Fill & Sign (forms and signatures)

The most common reason a student opens a PDF tool is a form someone sent.
Filling it should feel like typing into the paper itself, as in Preview.

- **Fields are soft tinted boxes** on the page: a light blue, lighter than
  the accent, which still means "selected" (the editor's outline and the
  frame in Edit Fields use the accent). Show Fields turns the tint off for
  reading. Read-only fields are grey; a required field without an answer
  has a small red asterisk at its corner.
- **Typing happens in the field.** A click (or Tab) opens an editor exactly
  over the field, at about the size the engine will draw the text, nearly
  white so the old answer doesn't show through. A comb puts one letter per
  box. A date has a small calendar button. Pop-up menus and lists are the
  system's own controls, so they open, scroll and type-to-find as menus do
  everywhere. Checkboxes and radio buttons change on a click or Space; the
  arrows move between the buttons of a group.
- **One answer, one undo step**: typing is sent when the field is left
  (Tab, Enter, a click elsewhere), never letter by letter.
- **The keyboard can fill a whole form.** Ctrl Alt E (Command Option E)
  goes to the next empty field; Tab and Shift Tab follow the form across
  pages and scroll each field into view; a live region says "Field 3 of
  12, Student ID, required". Escape gives the keyboard back to the page.
- **Mistakes are said beside the field**, in a few plain words ("This
  field is required. Type an answer."), not in a window message.
- **Honest limits, said where they matter**: a form with calculations says
  "This form has automatic calculations that Letters doesn’t run. Totals
  may need to be typed in."; an XFA form shows the engine's message in a
  banner and the tab; Find Fields says it only suggests, and that scanned
  pages need text recognition.
- **Two kinds of signature, two plain sentences.** Sign with a Picture is
  "a picture of your signature; it does not prove who signed". Sign with a
  Certificate is "a digital signature that lets others check the file has
  not changed since you signed". Self-signed is explained before a
  certificate is made. Passwords are never kept.
- **A signed document is protected from surprises.** A quiet banner says
  who signed and whether it has changed since. The first change asks, as a
  sheet like the unsaved changes question: "Changing this document will
  make its signature show as changed. Save a copy instead?" with Save a
  Copy… (prominent), Cancel and Edit Anyway at the leading edge, the way a
  mac sets a destructive choice apart.
- **Everything stays on this computer**: no server, no tracking of who
  opened, filled or signed; kept signatures live in the app's own folder.

### Protect (redaction, hidden information and passwords)

This is the tab where a mistake hurts a real person, so the safe path is
the easy path, and no sentence promises more than the engine does.

- **Two steps, like professional tools.** Mark Text, Mark Area and Find
  and Mark… only mark. A mark is a red, dashed, hatched box: it can't be
  mistaken for a finished redaction, which is a solid box drawn into the
  page. Red, not the accent: the accent means "selected", red means "to
  be removed". Marks can be picked, moved, resized (the shared frame) and
  removed, and are listed in Review Redactions…; they are never written
  to the file, and closing the document asks "Discard the 3 areas marked
  for redaction?" with Cancel as the default.
- **Every section says what the word means once** ("Redacting removes text
  and pictures from the file for good, so nobody can copy or uncover
  them.") and its line says that nothing is removed until you apply.
- **Review before applying.** A wide sheet lists, page by page and mark
  by mark, the exact text that goes, pictures removed or partly blanked,
  drawings, comments and form answers, the engine's own warnings (the
  same words in the title or on other pages, with a Find and Mark… button)
  and that fonts stay; beside it the page now and after, drawn from a
  redacted copy. Nothing changes while it is open.
- **The apply sheet** is an alert in the manner of a mac: "Apply 4
  redactions?", that it can't be brought back once saved, a "Not covered"
  box (words you didn't mark, hidden information, fonts), the box colour
  (Black or White) and an optional word in each box. Cancel has the focus;
  Apply Redactions is the prominent button.
- **Undo is honest**: it works until you save or close the document;
  saving deletes the earlier versions, and the tab says so. After
  applying, the next step is offered: Also Remove Hidden Information….
- **Remove Hidden Information…** lists only what the file has, each kind
  with a count, one sentence and what was found ("Author: Sam Lee").
  Safe kinds start ticked; comments and form answers don't, because they
  are often the point of the file.
- **Passwords say exactly what they do.** "The open password really
  encrypts the file with AES-256…"; "Restrictions are a request: anyone can
  still open the file, and some apps ignore them." The password is typed
  twice, with Show password and a strength hint that says what to do. A
  lock line at the top of the tab says when it takes effect ("Locked with
  AES-256 when you save."). A locked PDF opens at Unlock for Editing.
- **Keyboard**: Ctrl Alt R marks the selected text (or picks Mark Text);
  Ctrl Alt B picks Mark Area, and pressed again puts a box in the middle
  of the page with the focus on it, so the arrows move it, Alt with the
  arrows resizes it and Delete removes it. Control and Command with the
  same letters on a mac. Screen readers hear "3 areas marked on page 2."
- **On a narrow window** the floating inspector covers the page as usual;
  a Protect tool picked there keeps working once it is closed, and the
  sheets fit any window from 480 pixels up.

### Fonts (the Font Library and the font picker)

Students meet fonts in two moments: picking one while writing, and opening
a document that looks wrong because its fonts are missing. Both follow
Font Book and the Pages font menu.

- **The picker is a menu, not a dialog.** A compact button shows the
  chosen font in its own face; it opens a list with a search field on top:
  Favourites, Recent, In This Document, then All Fonts, each name drawn in
  its own font. Typing narrows it (typos forgiven), the arrows move, Enter
  chooses, Escape closes. The highlighted row uses the accent, as a
  selected menu item does. More Fonts… at the bottom opens the library.
- **The library is a sheet over the window** (Ctrl T, Command T on a mac,
  the Show Fonts key in mac apps): search at the top, two rows of chips
  (kind and language; where fonts come from, Embeddable, Favourites), a
  sample text field with Can Draw It and a size slider, then the list on
  the leading side and the selected family on the trailing side, like an
  inspector. An on chip uses the soft accent because it is a toggle's on
  state; Download is the one prominent button.
- **Content first**: each row is the family drawn in its own face with the
  person's sample (or its own name), with a quiet line of facts above it
  ("4 styles · Sans Serif · On This Computer"). The count under the search
  field is a live region and says what to try when nothing matches.
- **Plain sentences for licences**: "This font can be embedded in PDFs and
  documents." or "This font’s license doesn’t allow embedding; others will
  see a similar font unless they have it." The picker shows the second as
  a footnote under the button when such a font is chosen, never a dialog.
- **The internet is asked for, once.** The first Download or Show Preview
  shows a sheet saying exactly what is sent and to whom; everything else
  works offline.
- **Missing Fonts** lists each missing font, the stand-in Letters uses with
  one sentence on why (same letter widths, or only similar), and one
  button: Download the font, Download its stand-in, or Replace.
- **Adding fonts**: Add Fonts… and Add Folder…, or drop files on the
  library; a line at the bottom says what happened ("Added 2 fonts.",
  or why a file was refused).

### Compare

Comparing is looking, not editing, so it gets a screen of its own (like
the viewer) instead of a tab: the two versions are the content, and the
controls float.

- **Two ways in, one screen.** "Compare with Another Version…" in the
  document menu (and the File menu on a mac) starts with the open document
  as the newer version; "Compare Two PDFs…" on Recents starts empty. The
  first view names both versions in two cards, each with Choose…, and a
  Swap between them; dropping two PDFs fills both. Compare is the one
  prominent button, with the usual line under it saying what will happen
  ("Neither file changes.").
- **Progress you can stop.** A bar, the stage in words ("Comparing pages:
  120 of 500") and Stop.
- **Side by Side**: old on the left, new on the right, each a column of
  pages on the desk. The columns scroll together, the same passage level on
  both sides (Link Scrolling, pressed by default, turns it off). A page only
  one side has is a dashed empty sheet saying "Not in the old version".
- **Marks are soft, never colour alone.** Removed text is a light red tint
  on the left, added text light green on the right; moves are violet,
  formatting amber, layout teal, pictures orange, all quiet tints so the
  words stay readable. Each mark carries its word on a small light chip
  ("Added", "Moved", "Formatting"); on a crowded page only the selected
  change keeps its word. The selected change and its partner get the
  accent outline: here too the accent means "this one".
- **The Changes sidebar** says the summary in one line ("14 changes: 9
  text, 3 formatting, 2 layout"), offers the categories as checkboxes,
  Previous and Next Change (Alt ↑ and ↓), and every change as one sentence
  from the engine under its page. Identical documents say "These two
  documents are the same." even when the files differ in bytes.
- **Overlay** shows the engine's overlay picture (shared ink grey, removed
  red, added green) with one slider, Old at one end, New at the other and
  both in colour in the middle, and Flash, which alternates them. With
  reduced motion Flash never runs by itself; each press shows the other
  version.
- **Night Mode doesn't apply**: colours carry meaning here, and the pages
  stay white.
- **Save Report…** is a small sheet: PDF (with pictures of the pages and
  the changes marked), Web Page or Plain Text.

### Accessibility

The tab where the honest sentence matters most, because a mark of
accessibility is a promise to people who rely on it.

- **Four sections**: Check This Document, Fix Problems…, Mark as PDF/UA,
  Save Report…. The check's result starts with the engine's own score
  statement, which says what the number does and doesn't cover, then the
  findings in three groups: Must Fix, Should Fix, Check by Hand. Each
  finding opens to its explanation, why it matters, what was found, Show on
  Page (the spot gets an accent outline on the page) and Fix….
- **The wizard** goes one problem at a time with "3 of 11", Back, Skip and
  Done, and the small form each needs: a language by its name; a title; a
  picture shown above its alt text field with one sentence on what good
  alt text is and "Decorative — Skip for Screen Readers"; the heading
  ladder, indented by level and editable; a table shown with its header
  rows and columns; the reading order as numbered boxes on the page and
  the same numbers in a list that drags or moves with Alt and the arrows;
  contrast with the colours, the ratio in words and a sample of the
  suggested colour. After Add Tags Automatically, "Look Over These Tags"
  lists what the tagger guessed.
- **Mark as PDF/UA** stays off until the engine says every rule it can
  check passes, and then until the person ticks each check by hand
  themselves. Its line says exactly what the mark claims.

### The app's own accessibility

The app's colour tokens are checked by a unit test (`lib/a11y/tokenAudit.ts`)
against WCAG 2 in light, dark and increased contrast. To pass it the accent
fill, the accent text and the red are a little deeper than Apple's system
colours (`--accent` rgb(0 112 230) light, rgb(0 110 220) dark;
`--accent-label` rgb(0 90 190); `--danger` rgb(215 0 21); secondary text
at 0.6 opacity in light mode, 0.62 in dark), and with
increased contrast in dark mode text on the accent is black. See
docs/ACCESSIBILITY.md for the audit.

### Edit (the page's own text and pictures)

Fixing a typo in a PDF should feel like fixing it in Pages: click the
word and type.

- **One tool, the page as the target.** Edit Text and Images (Ctrl Alt
  W, Control Command W) makes every paragraph and picture a target. A
  paragraph shows a faint dashed outline under the pointer or the
  keyboard focus, never a coloured box; Tab moves through them.
- **Typing happens in the text.** The editor sits exactly over the
  paragraph, on its baseline, in its own font when the web view can load
  it (else the same font on this computer, else one of its kind), at its
  size and colour, nearly white so the old words don't show through. The
  accent outline says "this is being edited". Copy and paste are plain
  text. Clicking elsewhere or Ctrl Enter makes the change, one Undo;
  Escape drops it.
- **Never a silent substitution.** A letter the font can't draw is said
  under the editor before anything changes, and after the change a small
  glass card under the text names the font that drew it ("The original
  font can’t draw “é”, so Arial from this computer was used for it.").
- **Growing text asks.** When the new words need more lines than the
  paragraph had, the card says how much lower it now reaches and offers
  Shrink to Fit (the paragraph's own lines, smaller) and Let It Grow
  (prominent: it is what the person typed).
- **Pictures** get the shared frame (corners only: proportions stay; none
  on a turned picture, which can only be moved) and a small glass bar:
  Turn Left, Turn Right, Delete. The tab adds Bring to Front, Send to
  Back, Replace… with Fit, Fill or Stretch (each said in one sentence) and
  Save Image As…. Pictures can be pasted or dropped onto the page.
- **Add Text says what it is not**: real page text, unlike a Text Box,
  which floats on top as a comment. The tab says once, in a disclosure,
  what keeping the original font can and can't do.

### Scan (text recognition)

- **OCR is explained in the engine's own sentence**, where the tool is.
- **A quiet banner** on a document with scans nobody has read: one
  sentence, Not Now, and Make This Document Searchable (prominent), which
  opens the tab with those pages chosen. It never comes back for pages
  already read.
- **Ask for the language.** The installed languages are listed by name,
  the computer's own ticked; the line under them says the wrong language
  gets letters wrong. A second language can be ticked, with a sentence on
  the cost; a third can't.
- **Choices with one line each**: which pages (This Page, Selected, All,
  Scanned Only), Straighten tilted pages, Clean up speckles, Detect
  sideways pages.
- **The document stays usable while it reads.** The button becomes
  Cancel, a progress bar and "Reading page 3 of 14…" show how far it got,
  and screen readers hear each page. Cancel stops at once and adds
  nothing. The end is one Undo, with a summary in plain words ("Text found
  on 12 of 14 pages; 2 pages look blank."), the average confidence and
  the pages worth checking.
- **Show Recognized Text** draws the words read over the scan on a light
  wash, doubtful ones tinted yellow (not the accent); clicking one opens
  a small field to correct it.
- **New from Images…** is on Recents (a quiet tile beside Open a PDF),
  in the File menu and the Scan tab, and opening or dropping pictures
  offers it. A sheet lists the pictures in order (arrows to move them),
  Paper (Picture Size, Letter, A4, each explained), and Make It
  Searchable. The PDF opens unsaved; Save asks where, starting beside the
  pictures.

### Export To… (conversion)

One sheet, from the document menu, the File menu or Ctrl Shift E (Command
Shift E), the way Pages has one Export To… for every format.

- **Formats on the leading side, settings beside them.** Word Document,
  OpenDocument Text, Excel Workbook, PowerPoint Slides, Images, Web Page,
  Plain Text, as a list of radio buttons. A format this computer can't make
  stays in the list, greyed, with the reason under its name ("Coming soon."
  and the engine's sentence), so the list never changes shape.
- **Two ways, side by side.** Where a format comes two ways the choices are
  two cards: Exact Match and Easy to Edit (Keep Layout and Reading Order
  for plain text), each with the engine's own sentence about what it is
  good for. Exact Match is the default for Word (Settings > Files can
  change it); the chosen card gets the accent edge.
- **A few settings, then the save panel.** Images: PNG, JPEG or TIFF,
  resolution, colour or gray, quality for JPEG, all pages in one TIFF.
  Web Page: one file with pictures inside. Pages: All, or a range typed as
  "1-3, 7"; a mistake is said in a sentence before the panel opens.
- **While it works** a progress bar and Cancel; for Exact Match a sentence
  says beforehand that Word measures and checks the result and that this
  takes from about ten seconds to a minute and a half.
- **The result line** is the engine's report in plain words ("2 slides.
  Text is real text; 1 slide kept what couldn’t be rebuilt as a picture
  behind the text."). For Exact Match: the engine's own check sentence,
  "How close is it?" as a disclosure with the measured numbers, and each
  font that got a stand-in with the reason. Nothing claims "exact" that the
  check didn't confirm; without Word it says the result wasn't checked here.

### AI Tools and Find a Topic

- **Off, and out of sight, until turned on.** With the switch off (the
  default) there is no AI Tools button and no AI menu entry that does
  anything. Find a Topic is always there, under Find (a Topic… button in
  the find capsule, Ctrl Shift F), in the same trailing panel, with the line
  "Works on this computer. No AI, and nothing is sent anywhere."
- **The panel** sits on the trailing side like Explain, with a segmented
  control: Summarize (Short, Medium, Detailed), Key Points, Ask, Find a
  Topic, and a Pages field. Answers stream in as they are written, with a
  Stop button.
- **Who reads it is always on screen.** The panel's header names the
  provider and says "This stays on this computer." or, in bold, "Text you
  send goes to api.example.com."
- **Asking before sending.** Before any request that sends text away, an
  alert in the manner of a mac: "Send to api.example.com?", the engine's
  sentence ("This will send your question and the text of pages 3–7
  (about 2,400 words) to api.example.com."), the start of the text, a box
  "Don’t ask again for Online service and this document", and Cancel (which
  has the focus) beside Send. There is no box for "never ask".
- **Answers you can check.** Each quote in an answer is a small chip,
  "Page 4"; a click goes to the page and outlines the passage. "The
  document doesn’t say." is shown as it is. Quotes the engine couldn't find
  in the document are listed in red as not linked.
- **Errors** are the engine's sentences, with Try Again when trying again
  may help and Open Settings when it won't.

### Settings

A window of its own (Ctrl , or Command , the gear in Recents, the app
menu on a mac), with sections down the leading side like a mac's Settings:
General, Typing, Files, Fonts, Scanning, Accounts, AI Tools, Privacy.
Every change is kept at once; there is no Save button to forget. Sections
are a list of tabs the arrows move through. The AI Tools section starts
with the switch and one paragraph about what turning it on means; the key
field is a password field whose text goes straight to the system's
password store. Privacy lists everything that can leave the computer and
when, says there is no telemetry, and has the Default App paragraph with
Open Default Apps Settings… on Windows.

### Search All Documents, Go To and reading aids

- **One screen, one field.** Search All Documents (Ctrl Shift G, or
  Everywhere beside the Recents search field) is a screen of its own over
  the window, like Compare. Its one prominent control is the wide search
  capsule. Filter chips sit under it: kinds, a date pull-down, a folder
  pull-down and Titles and Headings. Chips are raised, never coloured,
  except the accent tint on a chosen one.
- **Results read like a book's index.** Each document shows its kind, its
  title, its folder and date, then its best pages with the words
  highlighted in the system's yellow, never the accent. The keyboard does
  everything: arrows move a row, Page Up and Down a document, Enter opens,
  Ctrl Enter opens in a new window, Escape clears and then goes back.
- **The words say what the index is.** "The index is kept on this
  computer. Nothing is sent anywhere." is always on the screen. Empty
  states offer the next step ("Add a folder to search…"); while indexing,
  "No Results Yet" says why.
- **Go To is a capsule, not a dialog.** Ctrl G drops a small capsule under
  the toolbar that understands "p 12", "xii", "50%", "ch 3", "#methods"
  and "last", lists the places as you type, and lists bookmarks when empty.
  Back and Forward (Alt Left and Right) follow the browser's habit.
- **Progress stays quiet.** One footnote-sized line at the bottom: page,
  percent, and time left only once the reader's own speed is known. The
  scrubber on the right edge appears only from 30 pages and shows the page
  and chapter in a bubble while dragged.
- **Organising looks like the Finder.** Ctrl or Command-click and
  Shift-click choose tiles, which get the accent ring. A bar at the bottom
  carries the actions; questions use the Finder's own words ("An item
  named … already exists in this location."), Delete always asks, and the
  toast's Undo really undoes.

### Code files

A small screen for the code and text files students hand in beside their
documents (HTML, C, Python, Java and the rest). It is an editor for
reading and fixing, not a programming environment: nothing is ever run.

- **Content first, as everywhere.** The code is a white sheet (dark in dark
  mode) on the desk; the toolbar, the Problems panel and the status bar
  float as glass. Toolbar: Recents, the file's name with its language under
  it and Save at the leading edge; Find, Go to Line and Comment in the
  centre; Problems (with a red count when there are errors), Open in VS
  Code, Show in Folder, New Window and Explain at the trailing edge.
- **The checks say how much to trust them.** The status bar names who
  checked the file: "Checked by gcc 16.2", "Checked inside Letters", or
  "Basic check only: no C compiler (gcc or clang) found on this computer."
  Its tooltip, Explain and the Problems panel say in one sentence what the
  check looks at. Every Explain entry about checking says that nothing is
  run.
- **Problems** are squiggles in the text, marks in the gutter, a count in
  the status bar and a list (Ctrl Shift M) with line, message, a plain hint
  for the commonest beginner mistakes under the raw message ("A semicolon
  is probably missing at the end of the line before this one.") and who
  said it. Errors are a red dot, warnings an amber triangle: shape as well
  as colour. F8 goes to the next one.
- **The editor's own keys** are the ones VS Code uses (Ctrl F with replace
  and a regexp switch, Ctrl G, Ctrl /, Alt Z to wrap, Ctrl D for the next
  match), because students meet VS Code first. More cursors are a Ctrl-click
  (Command-click) away, as CodeMirror sets it, and Alt-drag selects a
  rectangle.
  The language, the indentation, the encoding and line endings are quiet
  items in the status bar; the two choices are the system's pop-up menus.
- **Honest limits, said where they matter.** A file over 2 MB says that
  colours and checks are off to stay quick; over 32 MB it opens read-only.
  Saving a file whose bytes weren't all valid text asks first. Code found
  in a document starts with "Code copied into a document can lose
  characters… Treat these results as a hint", and each curly quote or
  non-breaking space it repaired is listed.
- **Not the default app for code.** Letters doesn't register itself for
  source files; they open through Open…, dropping, and Open With.

### Where Letters differs from the HIG on purpose

- **Toolbar buttons keep their text labels.** The HIG prefers symbols alone in
  toolbars. Letters is for first-year students who should never have to guess,
  and Apple's own design talk says a label generally beats an icon. Pages,
  Numbers and Keynote also ship with "Icon and Text" toolbars. Tooltips are
  still there for the longer description.
- **Explain is in the toolbar.** The HIG keeps help buttons out of toolbars.
  Explain is not a link to a manual: it opens a side panel describing the
  tools on screen, which makes it an inspector button, and those belong on the
  trailing edge.
- **The inspector's tabs are named for tasks, not formatting.** Pages names
  its inspector tabs after what is selected (Text, Arrange). A student comes
  to a PDF with a task (number the pages, fill in a form), so Letters names
  the tabs after tasks.
- **The icons are our own.** SF Symbols may only be used on Apple platforms,
  so Letters draws its own in the same spirit: one stroke weight, rounded
  ends, no outlines around them.

### Not done yet

- The macOS menu bar with every command is written but has never been run on
  a Mac. Windows has no menu bar on purpose: a permanent bar inside the window
  would box the document in, so the same commands are keyboard shortcuts there.
- Toolbar customisation.
- Full screen with the toolbar hidden.

## Sources

- [HIG: Toolbars](https://developer.apple.com/design/human-interface-guidelines/toolbars)
- [HIG: Materials](https://developer.apple.com/design/human-interface-guidelines/materials)
- [HIG: Color](https://developer.apple.com/design/human-interface-guidelines/color)
- [HIG: Typography](https://developer.apple.com/design/human-interface-guidelines/typography)
- [HIG: Layout](https://developer.apple.com/design/human-interface-guidelines/layout)
- [HIG: Sidebars](https://developer.apple.com/design/human-interface-guidelines/sidebars)
- [HIG: Buttons](https://developer.apple.com/design/human-interface-guidelines/buttons)
- [HIG: Writing](https://developer.apple.com/design/human-interface-guidelines/writing)
- [HIG: Motion](https://developer.apple.com/design/human-interface-guidelines/motion)
- [HIG: Designing for macOS](https://developer.apple.com/design/human-interface-guidelines/designing-for-macos)
- [WWDC25: Get to know the new design system (notes)](https://wwdcnotes.com/documentation/wwdc25-356-get-to-know-the-new-design-system/)
- [Pages User Guide: sidebars](https://support.apple.com/guide/pages/tan0870f78aa/mac)
- [MacRumors: Liquid Glass changes in macOS 27](https://macrumors.com/2026/06/09/macos-golden-gate-liquid-glass/)

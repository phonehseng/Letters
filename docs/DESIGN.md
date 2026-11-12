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
- **Kept content.** Things Letters can't edit (a chart, a text box, an
  equation) show as a quiet dashed chip, "Chart — kept as it is", saved
  back exactly. Deleting a range around one is refused with a sentence;
  Backspace next to one selects it first, so it is only ever deleted on
  purpose.
- **Pages** are an estimate of Word's: measured on screen, with a page gap
  where the next block no longer fits, the next page's header drawn in the
  gap, and footnotes under the text. The Explain entry and the status bar
  say so; Export as PDF uses Word or LibreOffice for exact pages.
- **The page stays white** in dark mode, as paper does in Pages; Night Mode
  in View darkens it.
- **A selected picture** gets a thin accent frame with eight round handles
  and, above it, one small glass bar: In Line | Free (a segmented control,
  the one thing most people want to change), the wrap when Free, Original
  Size, Fit Width, and one sentence saying what the current state means.
  While a handle is dragged only the picture changes size on screen, with
  its size in a small label under it; nothing else moves until release.
- **Free and Strict Word** is one quiet pill in the status bar (accent when
  Strict), mirrored at the top of the Document tab. Next to it the status
  bar names where page breaks come from ("Pages estimated", "Layout checked
  with Word a moment ago"), and in Strict mode one line says whether
  everything will look the same in Word.
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

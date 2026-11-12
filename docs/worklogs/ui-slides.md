# Worklog: the slide editor (ui)

## 2026-11-12 the slide editor screen

`.pptx` and `.odp` now open in a Slides screen of their own instead of as a
PDF made by Office; a file the engine can't read (an old `.ppt`, a damaged
file) still falls back to that PDF. The screen is built like the writer and
the code screen: the slide is the content on the desk, the toolbar, the
slide rail, the notes and the inspector float as glass.

The shell owns the deck (see shell-core-slides.md). The ui sends the
engine's operations (`operations.ts`); the engine names its operations in
camelCase but keeps rust's snake_case field names, so the fields are written
that way and a shell-core test parses the exact shapes the ui sends.

Drawing: each slide is the engine's own picture of it, shown as the pdf
renderer's tiles (`SlidePictureView`). A redrawn slide's tiles load out of
sight and replace the old ones only when every tile has arrived, so an edit
never flashes the slide white. The canvas draws only what the slide itself
doesn't: selection frames, the empty placeholders' "Click to add title"
prompts (powerpoint shows them while editing, never in the show), guides and
"kept as it is" chips.

Nothing moves when you click: a press only selects; it becomes a move after
the pointer travels three screen pixels (`canvasDrag.ts`). While dragging,
a copy of the object's part of the slide follows the pointer and the slide
itself waits for the release, which makes one operation and one undo step.
Moves snap to the slide's edges and centre and to other objects with the
writer's own snapping (`lib/writer/snapping.ts`); Alt moves freely. Resizing
a turned object reads the drag in the object's own axes and keeps the
opposite handle where it was, as powerpoint does. Turning snaps to 15°.

Typing: the field is laid over the box in the theme's font, size, colour,
alignment and line pitch (1.2 × size × spacing, the engine's measured
powerpoint rule), and the slide underneath is drawn once more without that
text, so only the field's words show. When the words are kept, the field
stays until the redrawn slide is on screen, so nothing blinks. The browser
breaks lines a little differently from the engine while typing; the slide
drawn afterwards is the engine's.

Keys are powerpoint's (windows and mac), checked on 2026-11-12 against
Microsoft's "Use keyboard shortcuts to create PowerPoint presentations" and
"... to deliver PowerPoint presentations", Apple's Keynote for Mac keyboard
shortcuts and Google's Slides shortcuts. Where they disagree: group is
Ctrl G (powerpoint) and also Ctrl Alt G (google slides) on windows, ⌥⌘G on a
mac (all three); bring to front and send to back have no windows shortcut in
powerpoint's list, so google slides' Ctrl Shift ↑ and ↓ are used; new slide
is ⌘⇧N on a mac (powerpoint and keynote) and Ctrl M on windows (powerpoint
and google slides). Not confirmed in any list: Hide Slide (keynote's ⇧⌘H is
powerpoint's Replace, so it has no key), Tab between objects and Enter to
type (powerpoint's behaviour, not in the published lists). The slide
screen reads its keys before the window does, because Ctrl G and Ctrl D mean
other things on other screens.

The slide show plays every slide but the hidden ones, full screen, with
powerpoint's show keys. Presenter view shows the slide, the next one, the
notes and a clock; with a second screen the slides go to an audience window
there (opened by the shell on the other monitor, kept in step by an event),
with one screen the presenter view is in this window.

Measured in the debug app on this machine (pp_title_bullets.pptx, 2 slides,
made by PowerPoint): the window's first slide tile was on screen 644 ms after
the page started, of which the open command took 376 ms (reading the deck,
drawing slide 1 and the Recents picture). Called again from the page,
opening took 37 ms and drawing both slides 17 ms; asking again with nothing
changed 4 ms. The typing field opened in 14 ms; keeping the words and
showing the redrawn slide took 29 ms. A new slide took 63 ms. With the
table deck duplicated to 120 slides, drawing every slide took 1431 ms
(11.9 ms a slide, debug build) and exporting all of them as one PDF 136 ms.

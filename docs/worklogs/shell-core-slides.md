# Worklog: presentations (shell-core)

## 2026-11-12 decks, undo, pictures, saving and recovery

Started `src/slides`, the shell's side of the slide editor, on top of the
slides engine (core/slides). Unlike the writer, whose ui holds the whole
model and sends it back to save, the shell owns each deck here: a deck's
model carries every package part's bytes for the byte-for-byte save, which
is far too much to send to the ui and back on every edit. The editor sends
the engine's own `edit::Operation`s and gets back which slides changed.

Undo keeps the inverse each operation returns (the engine snapshots what it
touched), so undoing is exact and untouched objects keep their xml. Unsaved
changes are counted as a position in the history, so undoing back to the
saved state counts as saved again; a new edit after undoing past the save
makes that state unreachable. The history keeps 200 steps.

Pictures: each slide is drawn by the engine into its own one-page pdf and
shown by the app's pdf renderer as ordinary tiles, so thumbnails, the
canvas and the slide show all use the existing tile path. One pdf per slide
means an edit redraws only the slides it changed; a redrawn slide is
written to a new file and the renderer's document is pointed at it (the
renderer keeps the old file open until then, so files are never rewritten
in place on windows). While text is typed into, the slide is drawn once
more with that text (or that table cell) left out, so the typing field's
words are the only ones on screen; the deck is put back exactly afterwards
and a test checks that.

What the editor is shown (`view.rs`, `view_text.rs`) is worked out with the
engine's inheritance and colour code, the way its renderer does: boxes come
from placeholders when a slide doesn't say, and text gets its effective
font, size and colour. Charts, SmartArt, embedded objects, effects,
arrowheads and recolouring get one honest sentence each: kept in the file,
not drawn or edited here.

New objects are made as powerpoint makes them: shapes with the theme's
style references (fill accent 1, outline accent 1 darkened 15 %, white body
font), text boxes that grow with their text, tables in powerpoint's default
style with a header row and bands. Their names use the id the engine will
give them. Recovery copies are the deck written in its own format plus a
small label, deleted on save or "Don't Save".

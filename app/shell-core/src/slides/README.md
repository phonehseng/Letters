# The shell's slides (app/shell-core/src/slides)

Open presentations (.pptx and .odp) for the slide editor, through the
slides engine (core/slides). The shell owns each deck: the editor sends the
engine's operations and gets back what changed. Why each piece is the way
it is: docs/worklogs/shell-core-slides.md.

| File | What it is for |
|---|---|
| `deck.rs` | one open deck: reading, edits with exact undo and redo, unsaved changes, saving |
| `find.rs` | Find and Replace across slide text, table cells and speaker notes |
| `mod.rs` | the module's parts, and the engine types the shell's commands use |
| `new_objects.rs` | the shapes, text boxes, tables and pictures Insert adds |
| `pictures.rs` | each slide drawn by the engine into a one-page pdf the app shows as tiles |
| `recovery.rs` | recovery copies of decks with unsaved changes |
| `sessions.rs` | every deck open in any window |
| `sessions_tests.rs` | tests of sessions.rs, through the calls the shell's commands make |
| `test_decks.rs` | the engine's test decks and scratch folders, for the tests |
| `view.rs` | what the editor is shown of a deck and of each slide's objects |
| `view_text.rs` | an object's text with its effective font, size and colour, and tables' cells |

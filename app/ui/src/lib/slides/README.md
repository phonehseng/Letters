# The slide editor (app/ui/src/lib/slides)

The screen a `.pptx` or `.odp` opens in, on top of the slides engine
(core/slides) through the shell (app/shell-core/src/slides). Why each piece is
the way it is: docs/worklogs/ui-slides.md.

| File | What it is for |
|---|---|
| `AudienceScreen.svelte` | the slide show's window on a second screen |
| `canvasDrag.ts` | what a drag on the slide means: move, resize, turn, nudge |
| `CellEditor.svelte` | typing in a table cell, with Tab to the next |
| `deckSession.svelte.ts` | one open deck: slides, selection, pictures, edits, undo, saving |
| `ExportSheet.svelte` | Export as PDF: slides, notes pages or handouts |
| `geometry.ts` | fitting, tiles, hit testing turned objects, turning and snapping |
| `inspector/` | the inspector's Slide, Arrange and Theme tabs |
| `NotesPane.svelte` | the speaker notes under the slide |
| `ObjectMarks.svelte` | empty placeholders' prompts, the hover outline, "kept as it is" chips |
| `operations.ts` | the engine's editing operations as the shell takes them |
| `railOrder.ts` | the rail's drops, selections, deletions and New Slide's layout |
| `SelectionFrame.svelte` | the accent frame with resize handles and the turning handle |
| `showOrder.ts` | which slides the show plays, its steps and its clock |
| `SlideCanvas.svelte` | the slide being edited, with selection, dragging and typing |
| `SlidePictureView.svelte` | a slide's picture as tiles, swapped only once the new one loaded |
| `SlideRail.svelte` | the numbered slide pictures on the leading side |
| `SlideShow.svelte` | the slide show and the presenter view |
| `slidesApi.ts` | every call into the shell, and the shapes it answers with |
| `slidesCommands.ts` | the commands and their keys (powerpoint's, checked against keynote and google slides) |
| `SlidesExplain.svelte` | Explain for the slide editor |
| `SlidesFind.svelte` | Find and Replace across slides, tables and notes |
| `SlidesInspector.svelte` | the Format inspector and its tabs |
| `SlidesScreen.svelte` | the screen: toolbar, rail, canvas, notes, inspector, show |
| `SlidesToolbar.svelte` | the floating toolbar |
| `slidesTools.ts` | the words for every tool: labels, tooltips, Explain |
| `TextEditor.svelte` | typing in place in a text box, in the theme's font and colour |
| `*.test.ts` | tests of the pure parts above |

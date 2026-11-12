# Worklog: the Edit tab (lib/edit)

What changed in this folder and why, newest last.

## 2026-11-11 Edit tab: text in place and pictures

Added the Edit tab's interface on top of core/edit. A session
(`editSession.svelte.ts`) owns the page tool, the paragraph being typed in
and the notices, because the viewer gives each tab one shared object to
read. Paragraphs and pictures are `data-page-ui` targets (`EditOverlay`)
rather than a page tool's hit testing, so hover outlines and Tab work
without a pointer. Typing becomes the smallest engine call
(`textDiff.ts`): an insert or delete keeps the rest of the line's own
glyphs, which a whole-paragraph re-set would not. The editor's top is
worked back from the first baseline with the measured font metrics
(`editorGeometry.ts`) so text does not jump when editing starts.
Engine ids are only valid until the next edit of a page, so every answer
carries the page's fresh objects, kept in `pageObjectsStore.svelte.ts`;
a `stale` answer makes the session find the same block or picture by
place and try once. Font notices are rephrased in `editSentences.ts` so
the person is always told which font drew missing letters.

## 2026-11-12 keep fresh ids before the viewer shows the change

The fresh objects and the new picture id were kept after the change
queue finished, but the viewer shows the new revision inside the queue,
so the session briefly saw a revision it did not know and closed an
editor opened meanwhile, and the picture frame lost its focus. They are
now kept inside the queued action.

## 2026-11-12 Add Text sends one object

`edit_add_text` took eight arguments, which clippy flags; the shell now
takes one `added` object, so `editApi.addText` sends it that way.

## 2026-11-12 split the picture tools out of the session

`editSession.svelte.ts` had grown past 600 lines. The picture operations
(select, move, resize, turn, restack, replace, save, add, paste, drop)
move to `pictureTools.svelte.ts`, which the session holds as `pictures`,
so each file covers one kind of object and stays readable.

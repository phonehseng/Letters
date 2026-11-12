# Worklog: the Compare screen (app/ui/src/lib/compare)

## 2026-11-11 pure modules for comparing

Added `compareApi.ts` (the shell's compare commands, typed after core/compare's
camelCase result), `changes.ts` (grouping by page pair, category filters, next
and previous change, the summary line, a tone and a word for every kind of
change), `scrollSync.ts` (one slot per page pair on each side, the engine's
`map_scroll` ported with the same test cases, following scroll),
`changeList.ts` (the virtual list's fixed row heights), `overlay.ts` (slider
layers and Flash), `progress.ts` and `setup.ts`, all tested in
`compare.test.ts`. Why: the screen's logic must be tested without a window,
and the scroll map has to agree exactly with the engine's.

## 2026-11-11 the Compare screen

Added `CompareScreen.svelte` and its parts: setup (older, newer, Swap, drop
two pdfs), progress with Stop, Side by Side (`CompareColumn`, `ChangeMarks`),
Overlay (`OverlayView`), the Changes sidebar (`ChangesList`, a listbox with
`aria-activedescendant` because its rows are virtualised), Options and Save
Report sheets, and `compareKeys.ts`. Why a dispatcher of its own: the screen
shows over a document that stays open, and registering on the app's
dispatcher would have overwritten the viewer's handlers for good.

## 2026-11-12 proof in the real app

Ran `cdp-compare/compare.mjs` (25 checks). The first run showed a moved
paragraph reported as "Changed … to …" plus "Added …" when new text was
inserted where it left; that is the engine (a replaced passage is never
offered to the move finder, core/compare text_diff.rs), so the test document
inserts its sentence elsewhere and the bug is reported. Made the stage a
`<main>` landmark after axe's region rule.

## 2026-11-12 readability pass

Moved the report's suggested path and words, and the overlay's slot sizes, out of `CompareScreen.svelte` into `report.ts` and `scrollSync.ts` (tested), so the screen stays under 500 lines and those rules have tests. Added a README listing the files, and an opening comment to the tests, as CONTRIBUTING.md asks.

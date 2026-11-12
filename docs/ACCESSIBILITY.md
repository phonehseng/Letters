# Accessibility of the app itself

What was checked in Letters' own interface during the accessibility stage
(November 2026), what passed, what failed and was fixed, what is left, and
what can't be known without a person who uses a screen reader.

The PDF checker and fix wizard (the Accessibility tab) are described in
CHANGELOG.md and ARCHITECTURE.md; this file is about the app.

## How it was checked

- **axe-core 4.10.2** (MPL-2.0, a development tool only, never shipped) was
  injected over the WebView2 debugging port into the real app (debug build)
  and run on 18 screens: Recents, Recents with Explain, the viewer, the
  document menu open, the View menu open, the sidebar's thumbnails and
  outline, Find, every inspector tab (Pages, Annotate, Fill & Sign,
  Protect, Accessibility, Edit, Scan), the viewer with Explain, the unsaved
  changes sheet, and the writer. The Compare screen (setup, side by side,
  overlay, the report sheet) and the Accessibility tab (checked, wizard,
  PDF/UA) were run from their own scripts.
- **Every screen in four colour modes**: light and dark, each with and
  without increased contrast (`prefers-contrast: more`), set through the
  debugging port.
- **Checks axe doesn't make**, by the same script: a keyboard walk (Tab
  forty times, noting each stop's name and whether its focus can be seen),
  controls smaller than 24 by 24 pixels (WCAG 2.2, 2.5.8), the text at 200%
  (the root font size doubled; the interface is in rem), and reduced motion
  (anything still moving with `prefers-reduced-motion: reduce`).
- **The colour tokens** of `app/ui/src/app.css` are checked by a unit test
  (`app/ui/src/lib/a11y/tokenAudit.ts`, in `a11y.test.ts`): text, accent
  text, the prominent button, the focus ring, red text and secondary text on
  controls, against WCAG 2 in all four modes, with see-through colours laid
  over what is under them.

Scripts: `cdp-compare/audit.mjs` (`before` injects this stage's starting
tokens; `after` uses the app as it is), `compare.mjs`, `a11y.mjs` in the
stage's scratch folder; results in `results-audit-before.json`,
`results-audit-after.json`.

## Results in numbers

| | Before | After |
|---|---|---|
| axe, light mode, 18 screens | 45 rule hits, 69 elements | 4 rule hits, 7 elements (6 in the writer) |
| axe, the other three modes | 186 elements | 17 elements (writer and the sidebar segment) |
| colour token pairs failing (unit test) | 6 of 40 (10 pairs × 4 modes) | 0 of 44 (an 11th pair added after axe found the sidebar control) |
| Tab escaping the unsaved changes sheet | yes | no |
| tab stops without a visible focus (viewer walk) | 1 (the pages) | 0 |
| controls under 24 × 24 px (Recents) | 25 | 1 |
| things moving with reduced motion | 0 | 0 |

The token failures before: accent text on a pressed toolbar button 3.85:1,
white on the prominent button 3.52:1 (light) and 3.23:1 (dark) and 2.15:1
(dark, more contrast), red text on the window 3.28:1 (light, both). The
Compare screen and Accessibility tab ended with 0 axe hits on every screen
checked.

## Fixed

| Problem | Fix | File |
|---|---|---|
| White text on the accent below 4.5:1 in light and dark | accent rgb(0 112 230) light, rgb(0 110 220) dark; black text on the accent in dark with more contrast | `app.css` |
| Accent text on a pressed toolbar button 3.85:1 | `--accent-label` rgb(0 90 190) | `app.css` |
| Red text 3.28:1 | `--danger` rgb(215 0 21) | `app.css` |
| Secondary text on a control on the glass just under 4.5:1 (the sidebar's Thumbnails / Outline control) | `--label-secondary` 0.6 (light), 0.62 (dark) | `app.css` |
| No main landmark or level-one heading in the viewer; the toolbar and pages outside any landmark | the stage is `<main>` with a hidden `<h1>` naming the document | `Viewer.svelte` |
| Recents' toolbar outside any landmark | in a `<header>` | `Home.svelte` |
| The pages showed no focus ring when they had the keyboard | an inset accent ring on `:focus-visible` | `Viewer.svelte` |
| Tab could leave the unsaved changes sheet into the window behind | the screen behind is `inert` while the question is open | `App.svelte` |
| Recents' "Remove from Recents" buttons 20 × 20 px | 24 × 24 px | `Home.svelte` |

The secondary-text change was measured by the token test; the app build
that would have re-run axe on it was blocked by other work in progress at
the time (see "Not verified").

## Still open (in other people's areas, listed rather than changed)

- **Writer** (`lib/writer`): colour contrast of "Add Header" / "Add
  Footer" (`.header-part > .add-part`) in every mode and of the Strict Word
  pill (`.mode`) in three; no main landmark; the ruler band, the body and the
  toolbar are outside landmarks; small targets: Add Header 95×23, Add Footer
  90×23, Strict Word 67×15, Show 25×13, "2 fonts missing" 68×13, the four
  status bar switches 17 px high; at 200% text the window scrolls sideways.
- **Viewer toolbar** (`ViewerToolbar.svelte`, `DocumentMenu.svelte`): the
  document name button is 18 px high and the page number field 14 px high
  (under the 24 px minimum; WCAG's spacing exception may cover the field,
  not the button).
- **Find** (`FindBar.svelte`): the search field is 18 px high.
- **Inspector, Pages tab** (`inspector/pages/CropTool.svelte`, the number
  fields): 20 px high.
- **Folders on Recents** (`FolderList.svelte`): Unpin is 20 × 20 px.
- **Text at 200%**: the toolbar labels (Sidebar, Page, Zoom Out, Zoom In,
  Find, View, Tools, Explain) and the document name are cut off rather than
  wrapping or dropping out, and Recents and the viewer scroll sideways. The
  toolbar's narrow-window rules (`toolbar.ts`) are by width in rem; at 200%
  they don't yet drop enough. The inspector's tab control (now seven tabs)
  cuts its labels at 200%.
- **Sheets don't give the focus back** (`protect/ProtectSheet.svelte`, used
  by Compare too): when one closes, the focus isn't returned to the button
  that opened it. Compare returns it itself; the Protect and Fill & Sign
  sheets don't.
- **Change marks on the Compare pages are pointer-only** on purpose (the
  Changes list is the keyboard's way in, as a listbox with Next and Previous
  Change). A screen reader user can't hear where on the page a change is
  beyond its sentence.

## Passed

- Every screen's keyboard walk reached every control, and focus was visible
  on every stop after the fixes (Recents 40 stops, viewer 13 toolbar stops,
  writer 40 stops).
- Menus (document, View) and the inspector's tabs use the arrow keys; the
  unsaved changes sheet traps Tab and Escape cancels.
- Nothing moves with reduced motion: no transitions or animations ran on
  any screen with `prefers-reduced-motion: reduce`, and Compare's Flash
  never runs by itself then.
- Live regions: the viewer's announcements, the Compare screen's "Change 4
  of 6. Page 1. …", the wizard's progress, the check's result.

## What can't be checked without a screen reader user

- Whether Narrator and VoiceOver read the viewer, the Changes listbox
  (`aria-activedescendant`), the wizard's steps and the reading order list
  in a way that makes sense, and whether the live regions are timely
  without being noisy. The announcements were read from the page, not heard.
- Whether the heading structure and landmarks help or clutter in practice.
- Whether the plain sentences are as clear spoken as they are read.
- High contrast (Windows' forced colours) was not tried; increased
  contrast was.

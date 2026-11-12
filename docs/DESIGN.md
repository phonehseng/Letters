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
| One accent | Toolbar symbols are monochrome. The accent appears on the "Open a PDF" tile, on pressed toggles (Night Mode, Tools, Explain), on selected thumbnails and the drop line when moving pages, on the one open tool's action button, and on focus rings. |
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
  segmented control at its top picks a tab. Today there is one, Pages; later
  stages add Annotate, Fill & Sign, Protect and Accessibility. The control
  shows even with one tab, so the place where tabs appear is already
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

### Side panels: sidebar, Tools and Explain

The sidebar is on the leading side; Tools and Explain share the trailing
side, Tools nearer the document. The rule (`app/ui/src/lib/sidePanels.ts`):
the document always keeps at least 26rem. Opening a panel that doesn't fit
closes the panel opened longest ago. On a wide window all three can be open;
on a medium one two; on a small one they take turns.

Explain is context-aware: with the inspector open it lists the open tab's
tools first, under "Pages tools", then the rest of the screen. Words a
first-year may not know (extract, split, merge, crop, header, watermark) are
explained there and in the open section.

Below 46rem the inspector stops narrowing the document and floats over it
like a sheet, full height, with its own close button (Escape closes it too).
At the 480px minimum window nothing overflows: the toolbar drops its
least important parts as before, and the floating inspector covers the page.

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

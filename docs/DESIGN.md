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
| One accent | Toolbar symbols are monochrome. The accent appears on the "Open a PDF" tile, on pressed toggles (Night Mode, Explain), and on focus rings. |
| Show less | The toolbar holds about nine actions in three groups. Night Mode and Two Pages sit in one View pull-down, as in Preview and Pages. The sidebar and the Explain panel are closed by default. Later stages add an inspector on the right that changes with the selection, as Pages does, instead of a ribbon. |
| Expected places | Leading: the sidebar button, back to Recents, then the title. Centre: zoom. Trailing: Find, View, New Window, then Explain (it opens a side panel, and inspector buttons go on the trailing edge). On a narrow window items leave in a fixed order, least important first (`app/ui/src/lib/toolbar.ts`). |
| Type scale | The macOS text styles, in rem so they scale with the system text size: large title 26/32, title 17/22, headline 13 bold, body 13/16, subheadline 11/14, footnote 10/13. Nothing is smaller than 10. Letter spacing follows Apple's tracking table. |
| Nested shapes | Toolbar groups have a 14px radius and 4px padding, so buttons inside have a 10px radius. The same sum is used wherever shapes nest. |
| Words | All labels and tooltips live in `app/ui/src/lib/tools.ts`. Title case verbs, "Open…" with an ellipsis, errors that say what to do. |
| Motion | 150 to 200 ms, only on hover and panel changes, off when the system asks for reduced motion. |
| Fits its platform | System font on each OS (San Francisco on macOS, Segoe UI Variable on Windows). Follows dark mode, reduced motion, reduced transparency (glass becomes solid) and increased contrast (Apple's higher-contrast blue). Every toolbar action has a keyboard shortcut. |

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

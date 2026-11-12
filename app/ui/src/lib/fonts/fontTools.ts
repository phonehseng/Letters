// the words of the font library and the font picker. they join the
// app's one list in ../tools.ts, so Explain and tooltips read them like
// every other tool's.

import type { ToolHelp } from "../tools";

/** what the one-time notice says before the first download. */
export function downloadNotice(host: string): string {
  return (
    `Downloading a font contacts ${host}, where Google Fonts keeps its open fonts. ` +
    "Only the request for that font is sent: no account, no name, nothing about your documents. " +
    "Fonts already on this computer never need the internet."
  );
}

export const fontTools = {
  fontLibrary: {
    label: "Font Library",
    tooltip: "Find, preview, add and download fonts",
    shortcut: "Ctrl T",
    whenToUse:
      "To find a font by name, kind or language, see what a font looks like with your own words, add fonts you have, or download one of thousands of free fonts.",
    fileEffect: "Nothing. Choosing a font in a document is a separate step.",
    meaning:
      "A font is a set of letter shapes. Fonts on this computer, fonts you add, fonts that come with Letters and free fonts to download are all listed together.",
  },
  addFonts: {
    label: "Add Fonts…",
    tooltip: "Add .ttf, .otf, .ttc, .woff or .woff2 files to Letters",
    whenToUse: "When a teacher or a class gives you a font file. Dropping font files on the Font Library does the same.",
    fileEffect:
      "Letters checks each file and keeps its own copy, for Letters only. Nothing is installed on the computer and the original file isn’t changed.",
  },
  addFontFolder: {
    label: "Add Folder…",
    tooltip: "Add every font file in a folder",
    whenToUse: "When fonts came in a folder, such as an unzipped download.",
    fileEffect: "Letters keeps its own copy of each font it can use. The folder isn’t changed.",
  },
  downloadFont: {
    label: "Download",
    tooltip: "Download this free font from the Letters library",
    whenToUse: "When you want a font that isn’t on this computer. The size is shown on the button.",
    fileEffect:
      "The font is saved in Letters’ own folder. Downloading contacts raw.githubusercontent.com and sends only the request for that font.",
    meaning: "These fonts are open source: free to use, share and embed in documents.",
  },
  useFont: {
    label: "Use in Documents",
    tooltip: "Make this font ready for documents in this window",
    whenToUse: "Before choosing a font you added or downloaded in a document, if it doesn’t show yet.",
    fileEffect: "Nothing. The font is only loaded for this window.",
  },
  removeFont: {
    label: "Remove from Letters",
    tooltip: "Delete Letters’ copy of this font",
    whenToUse: "For a font you added or downloaded and no longer need.",
    fileEffect: "Letters’ copy is deleted. Fonts installed on the computer are never removed by Letters.",
  },
  fontPicker: {
    label: "Font",
    tooltip: "Choose a font",
    whenToUse: "To change the font of the selected text. Type to find a font quickly.",
    fileEffect: "Only the working copy changes until you save.",
    meaning:
      "Each name is drawn in its own font. A font whose license doesn’t allow embedding shows a note, because people without it will see a similar font.",
  },
  moreFonts: {
    label: "More Fonts…",
    tooltip: "Open the Font Library",
    whenToUse: "When the font you want isn’t on this computer yet.",
    fileEffect: "Nothing.",
  },
  missingFonts: {
    label: "Missing Fonts",
    tooltip: "Fonts this document uses that aren’t on this computer",
    whenToUse: "When a document looks different from how it was written, often because it uses fonts from Microsoft Office.",
    fileEffect: "Nothing, until you choose Replace, which changes the working copy. The file on disk changes only when you choose Save.",
    meaning:
      "A metric-compatible font has the same letter widths as the one it stands in for, so lines and pages break in the same places.",
  },
} satisfies Record<string, ToolHelp>;

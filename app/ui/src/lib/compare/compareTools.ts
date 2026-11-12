// the words of the Compare screen's controls, joined into the one list in
// ../tools.ts, so labels, tooltips and Explain stay in one place.

import type { ToolHelp } from "../tools";

const NOTHING_CHANGES = "Nothing. Neither file is changed.";

export const compareTools = {
  compareWithVersion: {
    label: "Compare with Another Version…",
    tooltip: "See what changed between this document and an older version",
    whenToUse: "When you get a document back, such as an essay with a tutor’s edits, and want to see exactly what was changed.",
    fileEffect: NOTHING_CHANGES,
    meaning: "Comparing finds every difference in text, formatting, layout and pictures between two versions of a document.",
  },
  compareTwoPdfs: {
    label: "Compare Two PDFs…",
    tooltip: "Choose two versions of a document and see what changed",
    whenToUse: "To check two versions you have on this computer, such as a draft and a final copy. Dropping two PDFs here works too.",
    fileEffect: NOTHING_CHANGES,
  },
  compareChooseOlder: {
    label: "Choose Older…",
    tooltip: "Choose the older version of the document",
    whenToUse: "To pick the version to compare against: the one from before the changes.",
    fileEffect: NOTHING_CHANGES,
  },
  compareChooseNewer: {
    label: "Choose Newer…",
    tooltip: "Choose the newer version of the document",
    whenToUse: "To pick the version with the changes in it.",
    fileEffect: NOTHING_CHANGES,
  },
  compareSwap: {
    label: "Swap",
    tooltip: "Make the older version the newer one, and the other way round",
    whenToUse: "When the two versions were chosen the wrong way round, so additions show as removals.",
    fileEffect: NOTHING_CHANGES,
  },
  compareStart: {
    label: "Compare",
    tooltip: "Find what changed between the two versions",
    whenToUse: "Once both versions are chosen. A long document takes a moment; you can stop at any time.",
    fileEffect: NOTHING_CHANGES,
  },
  compareBack: {
    label: "Back",
    tooltip: "Leave the comparison",
    whenToUse: "When you are done looking at the changes. The documents stay as they are.",
    fileEffect: NOTHING_CHANGES,
  },
  compareSideBySide: {
    label: "Side by Side",
    tooltip: "Show the old version on the left and the new version on the right",
    whenToUse: "To read both versions at once. Changes are marked on both sides; click one to see its partner.",
    fileEffect: NOTHING_CHANGES,
  },
  compareOverlay: {
    label: "Overlay",
    tooltip: "Show the two versions laid over each other",
    whenToUse: "To spot small changes in layout or pictures. Removed ink is red, added ink is green, the rest is grey.",
    fileEffect: NOTHING_CHANGES,
  },
  compareLinkScrolling: {
    label: "Link Scrolling",
    tooltip: "Scroll both versions together",
    whenToUse: "On by default, so the same passage stays level on both sides. Turn it off to scroll each side on its own.",
    fileEffect: NOTHING_CHANGES,
  },
  compareChanges: {
    label: "Changes",
    tooltip: "Show or hide the list of changes",
    whenToUse: "To read every change as a sentence, grouped by page, and to show only some kinds of change.",
    fileEffect: NOTHING_CHANGES,
  },
  compareNextChange: {
    label: "Next Change",
    tooltip: "Go to the next change",
    shortcut: "Alt ↓",
    whenToUse: "To step through the changes one at a time. Both sides scroll to it.",
    fileEffect: NOTHING_CHANGES,
  },
  comparePreviousChange: {
    label: "Previous Change",
    tooltip: "Go back to the previous change",
    shortcut: "Alt ↑",
    whenToUse: "To go back to a change you passed.",
    fileEffect: NOTHING_CHANGES,
  },
  compareOptions: {
    label: "Options",
    tooltip: "Choose what counts as a change",
    whenToUse: "To leave out spacing and hyphenation, or to compare formatting, layout and pictures too. Then compare again.",
    fileEffect: NOTHING_CHANGES,
  },
  compareAgain: {
    label: "Compare Again",
    tooltip: "Compare the two versions again with these options",
    whenToUse: "After changing the options.",
    fileEffect: NOTHING_CHANGES,
  },
  compareBlend: {
    label: "Old to New",
    tooltip: "Fade from the old version, through both, to the new version",
    whenToUse: "In Overlay, to see one version, the other, or both with the changes in colour (the middle).",
    fileEffect: NOTHING_CHANGES,
  },
  compareFlash: {
    label: "Flash",
    tooltip: "Show the two versions one after the other",
    whenToUse: "In Overlay, to make a small change jump out. With reduced motion on, each press shows the other version once.",
    fileEffect: NOTHING_CHANGES,
  },
  compareSaveReport: {
    label: "Save Report…",
    tooltip: "Save the list of changes as a PDF, a web page or plain text",
    whenToUse: "To keep a record of what changed or share it, such as with a group you are writing with. The PDF has pictures of the pages with the changes marked.",
    fileEffect: "Writes a new file with the report. Neither document changes.",
  },
  compareStop: {
    label: "Stop",
    tooltip: "Stop comparing",
    whenToUse: "When a comparison of a long document takes longer than you want to wait.",
    fileEffect: NOTHING_CHANGES,
  },
} satisfies Record<string, ToolHelp>;

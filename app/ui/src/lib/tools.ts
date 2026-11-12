// the plain-english description of every tool, in one place.
//
// each tool has a label, a tooltip, and a longer entry for the explain
// panel that says when to use it and what it does to the file. keeping
// the words here (not scattered through the components) makes them easy
// to review as a set.
//
// writing rules (see docs/DESIGN.md): labels are verbs in title case, an
// ellipsis means the button opens something else, sentences are short,
// and nothing makes the reader guess.

export interface ToolHelp {
  label: string;
  /** a brief phrase shown when the pointer rests on the button */
  tooltip: string;
  /** the keys that do the same thing, if any, written for the reader */
  shortcut?: string;
  whenToUse: string;
  /** what happens to the file on disk */
  fileEffect: string;
  /** the tooltip when the tool can't be used right now, saying why */
  unavailable?: string;
}

const NOTHING_CHANGES = "Nothing. The file is not changed.";

// every edit changes the working copy, a private copy Letters keeps
// while the document is open. these words must stay exact: the person's
// file changes only through Save and Save As
const ONLY_THE_WORKING_COPY = "The file on disk changes only when you choose Save.";

export const tools = {
  recents: {
    label: "Recents",
    tooltip: "Close this document and show recent documents",
    whenToUse: "When you are done with this document and want to open another one in this window.",
    fileEffect: "Nothing. If there are changes you haven’t saved, Letters asks whether to save them first.",
  },
  open: {
    label: "Open…",
    tooltip: "Choose a PDF from this computer",
    shortcut: "Ctrl O",
    whenToUse: "To read a file that is not under Recents. Dragging a file onto the window does the same.",
    fileEffect: NOTHING_CHANGES,
  },
  newWindow: {
    label: "New Window",
    tooltip: "Open another window for a second document",
    shortcut: "Ctrl N",
    whenToUse: "To see two documents side by side, such as a reading and a set of notes.",
    fileEffect: NOTHING_CHANGES,
  },
  zoomOut: {
    label: "Zoom Out",
    tooltip: "Make the page smaller",
    shortcut: "Ctrl −",
    whenToUse: "To see more of the page at once.",
    fileEffect: NOTHING_CHANGES,
  },
  zoomIn: {
    label: "Zoom In",
    tooltip: "Make the page bigger",
    shortcut: "Ctrl +",
    whenToUse: "When the text is too small to read comfortably.",
    fileEffect: NOTHING_CHANGES,
  },
  fitWidth: {
    label: "Fit Width",
    tooltip: "Make the page as wide as the window",
    shortcut: "Ctrl 0",
    whenToUse: "For reading. The page fills the window, so there is no sideways scrolling.",
    fileEffect: NOTHING_CHANGES,
  },
  goToPage: {
    label: "Page",
    tooltip: "Type a page number, then press Enter",
    whenToUse: "To jump straight to a page, such as one named in a syllabus.",
    fileEffect: NOTHING_CHANGES,
  },
  nightMode: {
    label: "Night Mode",
    tooltip: "Show pages with a dark background",
    whenToUse: "For reading in a dark room. Photos look odd while it is on.",
    fileEffect: "Nothing. Only the screen changes. The file prints and shares as before.",
  },
  sidebar: {
    label: "Sidebar",
    tooltip: "Show or hide page thumbnails and the outline",
    whenToUse: "To see where you are in a long document and jump to another part of it.",
    fileEffect: NOTHING_CHANGES,
  },
  thumbnails: {
    label: "Thumbnails",
    tooltip: "Show a small picture of every page in the sidebar",
    shortcut: "Ctrl Alt 1",
    whenToUse: "To find a page by how it looks, such as the one with a diagram. Click a page to go to it.",
    fileEffect: NOTHING_CHANGES,
  },
  outline: {
    label: "Outline",
    tooltip: "Show the document's chapters and sections in the sidebar",
    shortcut: "Ctrl Alt 2",
    whenToUse: "To jump to a chapter or section by name. Not every PDF has an outline.",
    fileEffect: NOTHING_CHANGES,
  },
  find: {
    label: "Find",
    tooltip: "Find words in this document",
    shortcut: "Ctrl F",
    whenToUse: "To find every place a word or phrase appears, such as a term from a lecture.",
    fileEffect: NOTHING_CHANGES,
  },
  findNext: {
    label: "Next Match",
    tooltip: "Go to the next place the words appear",
    shortcut: "Enter",
    whenToUse: "After a search, to move through the matches one by one.",
    fileEffect: NOTHING_CHANGES,
  },
  findPrevious: {
    label: "Previous Match",
    tooltip: "Go back to the previous place the words appear",
    shortcut: "Shift Enter",
    whenToUse: "After a search, to go back to a match you passed.",
    fileEffect: NOTHING_CHANGES,
  },
  view: {
    label: "View",
    tooltip: "Choose how pages are shown",
    whenToUse: "To show two pages side by side, or to turn on Night Mode.",
    fileEffect: NOTHING_CHANGES,
  },
  twoPages: {
    label: "Two Pages",
    tooltip: "Show pages side by side in pairs, like an open book",
    shortcut: "Ctrl Alt 3",
    whenToUse: "On a wide screen, or for a book or magazine whose pages face each other.",
    fileEffect: NOTHING_CHANGES,
  },
  pinFolder: {
    label: "Pin a Folder…",
    tooltip: "Choose a folder whose PDFs always show here",
    whenToUse: "For a folder you open often, such as the one for a course. Its PDFs appear under Folders.",
    fileEffect: "Nothing. The folder and its files are not changed or moved.",
  },
  documentMenu: {
    label: "Document",
    tooltip: "Save the document, undo changes or rotate pages",
    whenToUse: "Click the document's name to save it, save a copy, undo a change or rotate the page you are on.",
    fileEffect: "Nothing by itself. Each command in the menu says what it does.",
  },
  save: {
    label: "Save",
    tooltip: "Save the changes to this document",
    shortcut: "Ctrl S",
    whenToUse: "When you are happy with your changes and want the file to keep them.",
    fileEffect:
      "Replaces the file on disk with the document as you see it now. The file is written in one step, so it is never left half saved.",
    unavailable: "There are no changes to save",
  },
  saveAs: {
    label: "Save As…",
    tooltip: "Save the document as a new file",
    shortcut: "Ctrl Shift S",
    whenToUse: "To keep the file you opened as it is and put your changes in a copy, or when the file is locked.",
    fileEffect:
      "Writes a new file with the document as you see it. The file you opened stays as it is on disk, and from then on Save writes to the new file.",
  },
  undo: {
    label: "Undo",
    tooltip: "Take back the last change",
    shortcut: "Ctrl Z",
    whenToUse: "Right after a change you didn’t mean to make. You can undo many changes in a row.",
    fileEffect: `Takes the change back in your working copy. ${ONLY_THE_WORKING_COPY}`,
    unavailable: "Nothing to undo",
  },
  redo: {
    label: "Redo",
    tooltip: "Make the change you just undid again",
    shortcut: "Ctrl Shift Z",
    whenToUse: "When you undid one change too many.",
    fileEffect: `Makes the change again in your working copy. ${ONLY_THE_WORKING_COPY}`,
    unavailable: "Nothing to redo",
  },
  rotateLeft: {
    label: "Rotate Left",
    tooltip: "Turn this page a quarter turn to the left",
    shortcut: "Ctrl L",
    whenToUse: "When a scanned page is on its side or upside down.",
    fileEffect: `Rotates the page in your working copy. ${ONLY_THE_WORKING_COPY}`,
  },
  rotateRight: {
    label: "Rotate Right",
    tooltip: "Turn this page a quarter turn to the right",
    shortcut: "Ctrl R",
    whenToUse: "When a scanned page is on its side or upside down.",
    fileEffect: `Rotates the page in your working copy. ${ONLY_THE_WORKING_COPY}`,
  },
  explain: {
    label: "Explain",
    tooltip: "Show what every button on this screen does",
    whenToUse: "Whenever a button is unclear, or to check whether it changes the file.",
    fileEffect: NOTHING_CHANGES,
  },
} satisfies Record<string, ToolHelp>;

export type ToolName = keyof typeof tools;

/** the tooltip text for a tool, with its keyboard shortcut when it has one. */
export function tooltipFor(name: ToolName): string {
  const tool: ToolHelp = tools[name];
  return tool.shortcut ? `${tool.tooltip} (${tool.shortcut})` : tool.tooltip;
}

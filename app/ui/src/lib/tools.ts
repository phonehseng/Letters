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
}

const NOTHING_CHANGES = "Nothing. The file is not changed.";

export const tools = {
  recents: {
    label: "Recents",
    tooltip: "Close this document and show recent documents",
    whenToUse: "When you are done with this document and want to open another one in this window.",
    fileEffect: NOTHING_CHANGES,
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

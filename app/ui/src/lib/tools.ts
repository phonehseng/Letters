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
  /** one sentence explaining a word a first-year may not know ("crop") */
  meaning?: string;
}

// tools that make new files leave the open document alone; say so exactly
const NEW_FILE_ONLY = "This document and its file on disk don’t change.";

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
  tools: {
    label: "Tools",
    tooltip: "Show the tools for changing pages",
    shortcut: "Ctrl Alt I",
    whenToUse: "To rotate, delete, add, crop, number or watermark pages, or to split and merge PDFs.",
    fileEffect: "Nothing by itself. Each tool says what it will do before you use it.",
  },
  pageSelection: {
    label: "Select Pages",
    tooltip: "Click a page to select it. Ctrl-click adds a page; Shift-click selects a run",
    shortcut: "Ctrl A",
    whenToUse:
      "Before using a page tool on more than one page. With nothing selected, tools work on the page you are reading.",
    fileEffect: NOTHING_CHANGES,
  },
  movePages: {
    label: "Move Pages",
    tooltip: "Drag pages in the sidebar to put them in a new order",
    shortcut: "Alt ↑ or Alt ↓",
    whenToUse: "When pages are in the wrong order, such as a scan fed in backwards. Dragging works too.",
    fileEffect: `Changes the order of pages in your working copy. ${ONLY_THE_WORKING_COPY}`,
  },
  rotatePages: {
    label: "Rotate",
    tooltip: "Turn pages a quarter or half turn",
    whenToUse: "When scanned pages are on their side or upside down.",
    fileEffect: `Rotates the pages in your working copy. ${ONLY_THE_WORKING_COPY}`,
  },
  deletePages: {
    label: "Delete Pages",
    tooltip: "Take pages out of this document",
    shortcut: "Delete",
    whenToUse: "To remove blank, repeated or unneeded pages.",
    fileEffect: `Takes the pages out of your working copy. Undo brings them back. ${ONLY_THE_WORKING_COPY}`,
    unavailable: "A PDF needs at least one page",
  },
  insertPages: {
    label: "Insert",
    tooltip: "Add a blank page or pages from another PDF",
    whenToUse: "To add a page for notes, or to bring in pages from another document, such as a cover sheet.",
    fileEffect: `Adds the pages to your working copy. The other PDF doesn’t change. ${ONLY_THE_WORKING_COPY}`,
  },
  extractPages: {
    label: "Extract",
    tooltip: "Save some pages as a new PDF",
    whenToUse: "To hand in or share only some pages, such as one chapter of a reader.",
    fileEffect: `Makes a new PDF with copies of the pages. ${NEW_FILE_ONLY}`,
    meaning: "Extracting copies pages out into a new PDF of their own.",
  },
  splitDocument: {
    label: "Split",
    tooltip: "Cut this PDF into several smaller PDFs",
    whenToUse: "When a file is too long or too big to upload, or to give each chapter its own file.",
    fileEffect: `Makes new PDFs in the folder you choose. Existing files there are never replaced. ${NEW_FILE_ONLY}`,
    meaning: "Splitting cuts one PDF into several smaller PDFs.",
  },
  mergeDocuments: {
    label: "Merge",
    tooltip: "Combine several PDFs into one",
    whenToUse: "To hand in one file made of several, such as an essay and its appendix.",
    fileEffect: "Makes one new PDF from the files, in the order listed. The files themselves don’t change.",
    meaning: "Merging joins whole PDFs, one after another, into a single new PDF.",
  },
  cropPages: {
    label: "Crop",
    tooltip: "Hide the edges of pages",
    whenToUse: "To trim a scanner’s dark border or wide margins.",
    fileEffect: `Hides the area outside the rectangle in your working copy. The hidden part is still in the file and can be shown again. ${ONLY_THE_WORKING_COPY}`,
    meaning:
      "Cropping hides the outside of a page; it doesn’t remove it. To remove something for good, use Redact under Protect.",
  },
  pageNumbers: {
    label: "Page Numbers",
    tooltip: "Add a number to every page",
    whenToUse: "When an assignment asks for numbered pages.",
    fileEffect: `Draws the numbers onto the pages of your working copy. Remove takes them off again. ${ONLY_THE_WORKING_COPY}`,
  },
  headerFooter: {
    label: "Header & Footer",
    tooltip: "Add a line of text to the top or bottom of pages",
    whenToUse: "To put your name, the course or the date on every page.",
    fileEffect: `Draws the text onto the pages of your working copy. Remove takes it off again. ${ONLY_THE_WORKING_COPY}`,
    meaning: "A header is a line of text at the top of every page; a footer is one at the bottom.",
  },
  watermark: {
    label: "Watermark",
    tooltip: "Add faint text or a picture across pages",
    whenToUse: "To mark a document as a draft or confidential, or to add a logo.",
    fileEffect: `Draws the watermark onto the pages of your working copy. Remove takes it off again. ${ONLY_THE_WORKING_COPY}`,
    meaning: "A watermark is faint text or a picture across the middle of a page, such as “Draft”.",
  },
  removeStamps: {
    label: "Remove",
    tooltip: "Take off page numbers, headers, footers or watermarks that Letters added",
    whenToUse: "To change your mind about something added with the tools above, even after saving.",
    fileEffect: `Takes them off the pages of your working copy. Only what Letters added is removed. ${ONLY_THE_WORKING_COPY}`,
  },
  explain: {
    label: "Explain",
    tooltip: "Show what every button on this screen and in the open tools does",
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

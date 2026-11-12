// the words of Search All Documents, the page finder and the reading and
// writing aids for long documents. they join the one list in ../tools.ts.
// writing rules in docs/DESIGN.md: title case verbs, short sentences,
// and always what happens to the file.

import type { ToolHelp } from "../tools";

const NOTHING_CHANGES = "Nothing. The file is not changed.";
const KEPT_LOCALLY = "Kept on this computer only. The file is not changed.";

export const libraryTools = {
  searchAllDocuments: {
    label: "Search All Documents",
    tooltip: "Search every document in your folders at once, down to the page",
    shortcut: "Ctrl Shift G",
    whenToUse: "When you remember a phrase but not which reading, lecture or draft it was in.",
    fileEffect: "Nothing. Letters keeps an index of the words on this computer, in its own data folder. Nothing is sent anywhere.",
    meaning: "An index is a list of every word and the pages it is on, like the index at the back of a book. It makes searching hundreds of documents instant.",
  },
  searchEverywhere: {
    label: "Everywhere",
    tooltip: "Search inside every indexed document, not just names in Recents",
    whenToUse: "When the words you want are inside a document rather than in its name.",
    fileEffect: NOTHING_CHANGES,
  },
  openInNewWindow: {
    label: "Open in New Window",
    tooltip: "Open this result in its own window",
    shortcut: "Ctrl Enter",
    whenToUse: "To keep the search results here while you read the document beside them.",
    fileEffect: NOTHING_CHANGES,
  },
  titlesOnly: {
    label: "Titles and Headings",
    tooltip: "Match words only in file names and headings",
    whenToUse: "To find the chapter or the document about something, rather than every mention of it.",
    fileEffect: NOTHING_CHANGES,
  },
  librarySettings: {
    label: "Search Settings…",
    tooltip: "Choose the folders that are searched and manage the index",
    whenToUse: "To add a folder of readings, leave a private folder out, or pause indexing.",
    fileEffect: "Nothing. These choices change only what Letters reads; your files are never changed.",
  },
  addLibraryFolder: {
    label: "Add Folder…",
    tooltip: "Search every document in a folder and the folders inside it",
    whenToUse: "For the folder where you keep a course's readings, or all your essays.",
    fileEffect: "Nothing. Letters reads the documents to index them and never changes them.",
  },
  dontIndexFolder: {
    label: "Don’t Index a Folder…",
    tooltip: "Never read the documents in this folder",
    whenToUse: "For private folders inside a folder you search, such as a diary or bank statements.",
    fileEffect: "Nothing. Anything already indexed from that folder is dropped from the index.",
  },
  pauseIndexing: {
    label: "Pause Indexing",
    tooltip: "Stop reading documents until you resume",
    whenToUse: "On battery, or when the computer feels busy. Searching keeps working with what is already indexed.",
    fileEffect: NOTHING_CHANGES,
  },
  rebuildIndex: {
    label: "Rebuild Index",
    tooltip: "Read every document again from the start",
    whenToUse: "If results seem out of date or missing.",
    fileEffect: "Nothing. Only the index in Letters’ data folder is made again.",
  },
  deleteIndex: {
    label: "Delete Index",
    tooltip: "Delete the index from this computer and pause indexing",
    whenToUse: "To free the space the index takes, or before giving the computer to someone else.",
    fileEffect: "Your documents are not touched. Only the index is deleted; Search All Documents finds nothing until you resume indexing.",
  },
  // "goToPage" is the toolbar's page number field; this is the Go To capsule
  goToPlace: {
    label: "Go To…",
    tooltip: "Go to a page, a printed page number, a percentage, a heading or a bookmark",
    shortcut: "Ctrl G",
    whenToUse: "To jump to “page 214”, “xii”, “50%”, “ch 3” or “#methods”. Clicking the page number does the same.",
    fileEffect: NOTHING_CHANGES,
  },
  historyBack: {
    label: "Back",
    tooltip: "Return to where you were before the last jump",
    shortcut: "Alt Left",
    whenToUse: "After following a reference to the endnotes or the bibliography.",
    fileEffect: NOTHING_CHANGES,
  },
  historyForward: {
    label: "Forward",
    tooltip: "Go forward again after going back",
    shortcut: "Alt Right",
    whenToUse: "To return to the reference you came back from.",
    fileEffect: NOTHING_CHANGES,
  },
  addBookmark: {
    label: "Add Bookmark",
    tooltip: "Remember this page under a name",
    shortcut: "Ctrl D",
    whenToUse: "For pages you will come back to: a key table, a formula, a quote for your essay.",
    fileEffect: "Bookmarks are kept on this computer and do not change the file, unless you choose Save Bookmarks into the PDF.",
  },
  bookmarks: {
    label: "Bookmarks",
    tooltip: "Show your bookmarks for this document",
    whenToUse: "To jump back to a page you marked.",
    fileEffect: KEPT_LOCALLY,
  },
  saveBookmarksToPdf: {
    label: "Save Bookmarks into the PDF",
    tooltip: "Add your bookmarks to the PDF’s own outline",
    whenToUse: "To share a PDF with your bookmarks in it, so other apps show them too.",
    fileEffect: "Adds outline entries to the PDF. The file on disk changes only when you choose Save, and Undo takes them out.",
    meaning: "A PDF’s outline is the list of chapters other apps show in their sidebar, often called bookmarks there.",
  },
  startFromBeginning: {
    label: "Start from the Beginning",
    tooltip: "Go to the first page and forget where you left off",
    whenToUse: "When you want to read the document again from the start.",
    fileEffect: KEPT_LOCALLY,
  },
  readingProgress: {
    label: "Reading Progress",
    tooltip: "The page you are on, how far through you are and the time left at your pace",
    whenToUse: "To plan a reading session. The time left is measured from your own reading on this computer.",
    fileEffect: KEPT_LOCALLY,
  },
  scrubber: {
    label: "Page Scrubber",
    tooltip: "Drag along the right edge to move through a long document",
    whenToUse: "To get to the middle of a 500 page book quickly. It shows the page and the chapter as you drag.",
    fileEffect: NOTHING_CHANGES,
  },
  findResultsList: {
    label: "Show All Matches",
    tooltip: "List every match in this document with the words around it",
    whenToUse: "To see where a word comes up across a long document, and jump to the right one.",
    fileEffect: NOTHING_CHANGES,
  },
  writingGoals: {
    label: "Writing Goals…",
    tooltip: "Set a word target for the document and for this session",
    whenToUse: "For essays with a word limit, or to keep a daily writing habit. Progress shows quietly in the status bar.",
    fileEffect: KEPT_LOCALLY,
  },
  focusMode: {
    label: "Focus",
    tooltip: "Hide everything but the page, and bring the paragraph you are writing forward",
    shortcut: "Ctrl Shift Enter",
    whenToUse: "For long stretches of writing without distraction. Escape brings everything back.",
    fileEffect: NOTHING_CHANGES,
  },
  typewriterScrolling: {
    label: "Typewriter Scrolling",
    tooltip: "Keep the line you are typing in the middle of the window",
    whenToUse: "So your eyes stay in one place while you write.",
    fileEffect: NOTHING_CHANGES,
  },
  saveVersion: {
    label: "Save a Version…",
    tooltip: "Keep a named copy of the document as it is now",
    whenToUse: "Before a big rewrite, or at the end of each draft, so you can go back.",
    fileEffect: "Copies the saved file into Letters’ data folder on this computer. The document itself is not changed.",
  },
  versions: {
    label: "Versions",
    tooltip: "See and restore the versions you saved",
    whenToUse: "To go back to an earlier draft. Restoring keeps the current file as a version first, so nothing is lost.",
    fileEffect: "Restore replaces the file with the version you choose, after keeping the current one as a version.",
  },
  // organising documents (lib/organise)
  moveToFolder: {
    label: "Move to Folder…",
    tooltip: "Move the chosen documents into another folder",
    shortcut: "Ctrl Shift M",
    whenToUse: "To file downloads into a course folder. Undo moves them back.",
    fileEffect: "Changes where the files live on this computer. Nothing is uploaded. Recents, search, bookmarks and versions follow them, and an open document keeps working.",
  },
  copyToFolder: {
    label: "Copy to…",
    tooltip: "Put a copy of the chosen documents in another folder",
    whenToUse: "To keep a document in two places, such as a course folder and a submission folder.",
    fileEffect: "Makes new files in the folder you choose. The originals stay where they are.",
  },
  renameDocument: {
    label: "Rename",
    tooltip: "Give the document a new file name",
    shortcut: "F2",
    whenToUse: "To give a download like “document(3).pdf” a name you will recognise.",
    fileEffect: "Changes the file’s name on this computer. Its contents are not changed.",
  },
  revealDocument: {
    label: "Show in File Explorer",
    tooltip: "Show the file in its folder",
    whenToUse: "To attach the file to an email or find the files around it.",
    fileEffect: NOTHING_CHANGES,
  },
  removeFromRecents: {
    label: "Remove from Recents",
    tooltip: "Take the documents off this list. The files are not deleted",
    whenToUse: "To tidy Recents without touching the files.",
    fileEffect: NOTHING_CHANGES,
  },
  moveToRecycleBin: {
    label: "Move to Recycle Bin",
    tooltip: "Move the chosen documents to the Recycle Bin (the Trash on a Mac)",
    shortcut: "Delete",
    whenToUse: "For documents you no longer need. Letters asks first.",
    fileEffect: "Moves the files to the system’s Recycle Bin, where they can be put back. Letters never deletes a file for good.",
  },
  manuscriptCounts: {
    label: "Word Counts by Section",
    tooltip: "Show how many words each chapter and section holds",
    whenToUse: "To balance the length of chapters, or check a section against its limit.",
    fileEffect: NOTHING_CHANGES,
  },
} satisfies Record<string, ToolHelp>;

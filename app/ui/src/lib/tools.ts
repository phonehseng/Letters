// the plain-english description of every tool, in one place.
//
// each tool has a short tooltip and a longer "what's this?" entry that
// says when to use it and what it does to the file. keeping the words
// here (not scattered through the components) makes them easy to review.

export interface ToolHelp {
  label: string;
  /** one short sentence, shown when the pointer rests on the button */
  tooltip: string;
  whenToUse: string;
  /** what happens to the file on disk */
  fileEffect: string;
}

const NOTHING_CHANGES = "Nothing. Your file is not changed.";

export const tools = {
  home: {
    label: "Documents",
    tooltip: "Close this document and go back to your files",
    whenToUse: "When you are done reading and want to open something else in this window.",
    fileEffect: NOTHING_CHANGES,
  },
  open: {
    label: "Open",
    tooltip: "Choose a PDF from your computer",
    whenToUse: "To read a file that is not in your recent list. You can also drag a file onto the window.",
    fileEffect: NOTHING_CHANGES,
  },
  newWindow: {
    label: "New window",
    tooltip: "Open another window for a second document",
    whenToUse: "When you want two documents side by side, for example a reading and your notes.",
    fileEffect: NOTHING_CHANGES,
  },
  zoomOut: {
    label: "Zoom out",
    tooltip: "Make the page smaller (Ctrl and minus)",
    whenToUse: "To see more of the page at once.",
    fileEffect: NOTHING_CHANGES,
  },
  zoomIn: {
    label: "Zoom in",
    tooltip: "Make the page bigger (Ctrl and plus)",
    whenToUse: "When the text is too small to read comfortably.",
    fileEffect: NOTHING_CHANGES,
  },
  fitWidth: {
    label: "Fit width",
    tooltip: "Make the page as wide as the window (Ctrl and 0)",
    whenToUse: "The easiest size for reading: no sideways scrolling.",
    fileEffect: NOTHING_CHANGES,
  },
  goToPage: {
    label: "Page",
    tooltip: "Type a page number and press Enter to jump there",
    whenToUse: "When a syllabus or a friend tells you to look at a certain page.",
    fileEffect: NOTHING_CHANGES,
  },
  nightMode: {
    label: "Night mode",
    tooltip: "Show pages with a dark background",
    whenToUse: "For reading in a dark room. Photos will look odd while it is on.",
    fileEffect: "Nothing. It only changes how pages look on your screen, not when printed or shared.",
  },
  help: {
    label: "What's this?",
    tooltip: "Explains every button on this screen",
    whenToUse: "Whenever you are not sure what a button does or whether it will change your file.",
    fileEffect: NOTHING_CHANGES,
  },
} satisfies Record<string, ToolHelp>;

export type ToolName = keyof typeof tools;

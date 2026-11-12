// the plain-english descriptions of the code editor's tools, joined into
// the app's one list in ../tools.ts. every entry that checks code says
// that nothing is run, because a student should never have to wonder.

import type { ToolHelp } from "../tools";

const NOTHING_RUNS = "Letters never runs your program; it only reads it.";

export const codeTools = {
  codeSave: {
    label: "Save",
    tooltip: "Save your changes to this file",
    shortcut: "Ctrl S",
    whenToUse: "When you want the file on disk to have your edits.",
    fileEffect: "Replaces the file with your edits, in the same encoding and line endings it had. Letters writes a new copy first, so a crash never leaves half a file.",
  },
  codeSaveAs: {
    label: "Save As…",
    tooltip: "Save a copy under another name or in another folder",
    shortcut: "Ctrl Shift S",
    whenToUse: "To keep the original as it is, or to give a new file a name and an extension.",
    fileEffect: "Writes a new file. The window then shows the new file; the original stays as it was.",
  },
  codeFind: {
    label: "Find",
    tooltip: "Find and replace in this file",
    shortcut: "Ctrl F",
    whenToUse: "To find a word, or replace every use of a name. Turn on “regexp” to search with a regular expression.",
    fileEffect: "Nothing until you save. Replacing changes the text on screen; Undo takes it back.",
    meaning: "A regular expression is a search pattern: \\d+ finds any number, for example.",
  },
  codeGoToLine: {
    label: "Go to Line",
    tooltip: "Jump to a line number",
    shortcut: "Ctrl G",
    whenToUse: "When a compiler or a teacher names a line, such as “error on line 42”.",
    fileEffect: "Nothing. Only the cursor moves.",
  },
  codeToggleComment: {
    label: "Comment",
    tooltip: "Turn the selected lines into comments, or back",
    shortcut: "Ctrl /",
    whenToUse: "To switch some lines off while you try something, without deleting them.",
    fileEffect: "Adds or removes the comment marks (// or #) on screen. The file changes when you save.",
    meaning: "A comment is text the computer ignores, written for people reading the code.",
  },
  codeWrap: {
    label: "Wrap Lines",
    tooltip: "Fold long lines so they fit the window",
    shortcut: "Alt Z",
    whenToUse: "For long lines of text or HTML that run off the side.",
    fileEffect: "Nothing. Only the screen changes; no line breaks are added to the file.",
  },
  codeProblems: {
    label: "Problems",
    tooltip: "Show the list of mistakes the checks found",
    shortcut: "Ctrl Shift M",
    whenToUse: "To see every mistake at once and go to each. F8 jumps to the next one.",
    fileEffect: `Nothing. ${NOTHING_RUNS}`,
  },
  codeCheckNow: {
    label: "Check Now",
    tooltip: "Check this file for mistakes again",
    whenToUse: "After installing a compiler, or to check without waiting. Files are also checked a moment after you stop typing, and when you save.",
    fileEffect: `Nothing. A compiler, when installed, reads a copy in a temporary folder and only checks the syntax. ${NOTHING_RUNS}`,
    meaning: "A compiler turns code into a program. Here it is only asked whether the code is written correctly.",
  },
  codeIndentation: {
    label: "Indentation",
    tooltip: "Choose tabs or spaces, and how many",
    whenToUse: "When the file uses another style than the one detected, or your teacher asks for one.",
    fileEffect: "Only the next lines you indent change. Existing lines stay as they are.",
  },
  codeOpenInVsCode: {
    label: "Open in VS Code",
    tooltip: "Open this file in Visual Studio Code at the cursor's line",
    whenToUse: "When you want a full programming editor, to run or debug the code.",
    fileEffect: "Nothing. Save first, or VS Code shows the file as it was last saved.",
    unavailable: "Visual Studio Code isn’t installed on this computer. Install it from code.visualstudio.com to use this.",
  },
  codeReveal: {
    label: "Show in Folder",
    tooltip: "Show this file in File Explorer (the Finder on a Mac)",
    whenToUse: "To find the file to attach, upload or copy.",
    fileEffect: "Nothing. The folder opens with the file selected.",
  },
  exportCodeFile: {
    label: "Export as Code File…",
    tooltip: "Save a copy as Markdown, HTML, LaTeX and other text formats",
    shortcut: "Ctrl Alt X",
    whenToUse: "To use a document’s text in a programming project, a LaTeX template or a website.",
    fileEffect: "Makes a new file (pictures go in an “images” folder beside it). This document doesn’t change.",
  },
  checkDocumentCode: {
    label: "Check Code in This Document",
    tooltip: "Find code in this document and check it for mistakes",
    shortcut: "Ctrl Alt K",
    whenToUse: "When an assignment or a handout has code in it and you want to know whether it is written correctly.",
    fileEffect: `Nothing. Code in monospaced fonts is read and checked like a code file. ${NOTHING_RUNS} Results are a hint: a document can lose characters.`,
  },
} satisfies Record<string, ToolHelp>;

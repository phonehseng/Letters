// the words of the Edit tab's tools: label, tooltip, shortcut, when to
// use it and exactly what it does to the file. tools.ts adds them to the
// app's one list, so Explain shows them like every other tool.

import type { ToolHelp } from "../tools";

const ONLY_THE_WORKING_COPY = "The file on disk changes only when you choose Save.";

export const editTools = {
  editText: {
    label: "Edit Text and Images",
    tooltip: "Change the words on the page in place, or move and resize its pictures",
    shortcut: "Ctrl Alt W",
    whenToUse:
      "To fix a typo, a date or a name in a PDF someone sent you, or to move a picture. Click a paragraph to type in it; click a picture to select it.",
    fileEffect: `Rewrites only the words you changed in your working copy, keeping the original font for every letter it holds. Each change is one Undo. ${ONLY_THE_WORKING_COPY}`,
    meaning:
      "Editing in place changes the page’s own text, the way the author would have, rather than covering it with a comment.",
  },
  editTextStyle: {
    label: "Text Style",
    tooltip: "Change the size, colour or font of the text you are editing",
    whenToUse: "After clicking into a paragraph: select letters to change only those, or nothing to change the whole paragraph.",
    fileEffect: `Changes the look of that text in your working copy; text after it on the line moves to make room. ${ONLY_THE_WORKING_COPY}`,
  },
  deletePageText: {
    label: "Delete Text",
    tooltip: "Remove the paragraph you are editing from the page",
    whenToUse: "To take out a whole paragraph or caption. To remove a few words, select them while editing and press Delete.",
    fileEffect: `Takes the text out of the page in your working copy; its space stays empty. Undo brings it back. ${ONLY_THE_WORKING_COPY}`,
  },
  addPageText: {
    label: "Add Text",
    tooltip: "Write new text into the page itself",
    whenToUse: "To add a word, a line or a label that should be part of the page, such as a missing date.",
    fileEffect: `Writes the text into the page of your working copy, in the font you choose. ${ONLY_THE_WORKING_COPY}`,
    meaning:
      "Add Text writes real page text; a Text Box under Annotate is a comment that floats on top and other readers can hide or remove.",
  },
  pageImages: {
    label: "Pictures",
    tooltip: "Move, resize, turn, replace, restack, save or delete the selected picture",
    whenToUse: "After clicking a picture on the page while Edit Text and Images is on. Drag it, or use its corners; the arrows nudge it.",
    fileEffect: `Changes only that picture’s place or content in your working copy. Save Image As… makes a new file and changes nothing. ${ONLY_THE_WORKING_COPY}`,
  },
  addPageImage: {
    label: "Add Image",
    tooltip: "Put a PNG or JPEG picture into the page",
    whenToUse: "To add a photo, a diagram or a logo as part of the page. Choose a file, paste one, or drop one on the page.",
    fileEffect: `Adds the picture to the page of your working copy. The picture file doesn’t change. ${ONLY_THE_WORKING_COPY}`,
  },
} satisfies Record<string, ToolHelp>;

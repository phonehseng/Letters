// the plain-english descriptions of the slide editor's tools, joined into
// the app's one list in ../tools.ts: a label, a tooltip, when to use it,
// what it does to the file, and the word explained where a first-year may
// not know it. the shortcuts are the windows ones; slidesCommands.ts has
// the mac ones, which the screen shows on a mac.

import type { ToolHelp } from "../tools";

const UNTIL_SAVED = "The file on disk changes only when you choose Save. Undo takes it back.";
const NOTHING_CHANGES = "Nothing. The file is not changed.";

export const slidesTools = {
  sSlides: {
    label: "Slides",
    tooltip: "Show or hide the list of slides",
    whenToUse: "To see every slide, drag one to a new place, or right-click one to duplicate, hide or delete it.",
    fileEffect: NOTHING_CHANGES,
  },
  sDocument: {
    label: "Presentation",
    tooltip: "Save, Save As, Export as PDF, Undo and Redo",
    whenToUse: "To keep your changes, make a copy in the other format, or make a PDF to hand in.",
    fileEffect: "Save writes the file; the rest say what they do.",
  },
  sSave: {
    label: "Save",
    tooltip: "Save your changes to this presentation",
    shortcut: "Ctrl S",
    whenToUse: "When the file on disk should have your edits.",
    fileEffect: "Writes the file. Parts you didn’t change (animations, charts, effects) are written back exactly as they were, byte for byte.",
    unavailable: "There are no changes to save.",
  },
  sSaveAs: {
    label: "Save As…",
    tooltip: "Save a copy as PowerPoint (.pptx) or OpenDocument (.odp)",
    shortcut: "Ctrl Shift S",
    whenToUse: "To keep the original as it is, or to give the presentation to someone who uses the other app.",
    fileEffect: "Writes a new file in the format its name ends in. The window then shows the new file.",
    meaning: ".pptx is PowerPoint’s format; .odp is the OpenDocument format LibreOffice and Keynote can open.",
  },
  sExportPdf: {
    label: "Export as PDF…",
    tooltip: "Make a PDF of the slides, the notes pages or handouts",
    whenToUse: "To hand in or print the presentation, or send it to someone without PowerPoint.",
    fileEffect: "Writes a new PDF. The presentation doesn’t change.",
    meaning: "Notes pages put each slide above its speaker notes. Handouts put 2, 3, 4 or 6 slides on a page, three with lines for writing.",
  },
  sNewSlide: {
    label: "New Slide",
    tooltip: "Add a slide after this one, from a layout",
    shortcut: "Ctrl M",
    whenToUse: "To add the next slide. Pick a layout to start with a title, a title and text, or nothing.",
    fileEffect: UNTIL_SAVED,
    meaning: "A layout is a slide’s starting arrangement: where its title and text boxes sit.",
  },
  sDuplicateSlide: {
    label: "Duplicate Slide",
    tooltip: "Copy this slide, just after it",
    shortcut: "Ctrl Shift D",
    whenToUse: "To make a slide like this one and change a few things.",
    fileEffect: UNTIL_SAVED,
  },
  sDeleteSlide: {
    label: "Delete Slide",
    tooltip: "Remove the selected slides",
    shortcut: "Delete",
    whenToUse: "When a slide is no longer needed. Undo brings it back until you close.",
    fileEffect: UNTIL_SAVED,
  },
  sHideSlide: {
    label: "Hide Slide",
    tooltip: "Skip this slide in the slide show, but keep it",
    whenToUse: "For a backup slide you may need for questions.",
    fileEffect: UNTIL_SAVED,
    meaning: "A hidden slide stays in the file and in exports of notes, but the slide show passes over it.",
  },
  sLayout: {
    label: "Layout",
    tooltip: "Rearrange this slide on another layout",
    whenToUse: "When a slide needs two columns, or only a title.",
    fileEffect: "The title and text keep their words and move to the new layout’s places. " + UNTIL_SAVED,
  },
  sInsert: {
    label: "Insert",
    tooltip: "Add a text box, a shape, a picture or a table",
    whenToUse: "To add something to the slide. It lands in the middle, selected, so the arrows can move it.",
    fileEffect: UNTIL_SAVED,
  },
  sTextBox: {
    label: "Text Box",
    tooltip: "Add a box to type in",
    whenToUse: "For words outside the slide’s title and text, such as a label or a source.",
    fileEffect: UNTIL_SAVED,
  },
  sShape: {
    label: "Shape",
    tooltip: "Add a rectangle, an oval, an arrow or a star",
    whenToUse: "To point at something or frame it. Shapes take the theme’s colours; type to put words in one.",
    fileEffect: UNTIL_SAVED,
  },
  sPicture: {
    label: "Picture…",
    tooltip: "Add a picture from this computer",
    whenToUse: "For a photo, a chart saved as a picture, or a screenshot.",
    fileEffect: "The picture is copied into the presentation. " + UNTIL_SAVED,
  },
  sTable: {
    label: "Table",
    tooltip: "Add a table with rows and columns",
    whenToUse: "To compare things side by side. Double-click a cell to type in it.",
    fileEffect: UNTIL_SAVED,
  },
  sTableRows: {
    label: "Rows and Columns",
    tooltip: "Add or remove a row or a column of the selected table",
    whenToUse: "When the table needs one more row, or one column fewer.",
    fileEffect: "Merged cells stay merged. " + UNTIL_SAVED,
  },
  sArrange: {
    label: "Arrange",
    tooltip: "Bring to front, send to back, group and position",
    whenToUse: "When one object hides another, or to move several as one.",
    fileEffect: UNTIL_SAVED,
    meaning: "Objects lie on top of each other like paper cut-outs; Bring to Front puts one on top.",
  },
  sGroup: {
    label: "Group",
    tooltip: "Join the selected objects so they move as one",
    shortcut: "Ctrl G",
    whenToUse: "For a picture and its label, or a diagram made of shapes. Ungroup (Ctrl Shift G) takes them apart.",
    fileEffect: UNTIL_SAVED,
  },
  sTheme: {
    label: "Theme",
    tooltip: "Change the presentation’s colours and fonts",
    whenToUse: "To change every slide’s look at once: everything that follows the theme follows.",
    fileEffect: UNTIL_SAVED,
    meaning: "A theme is the set of colours and fonts every slide draws from. Text and shapes in “theme” colours change with it; colours picked by hand don’t.",
  },
  sFormat: {
    label: "Format",
    tooltip: "Show the slide, object, table and theme settings",
    shortcut: "Ctrl Alt I",
    whenToUse: "To set an exact position or size, change a fill, pick a layout or change the theme.",
    fileEffect: NOTHING_CHANGES,
  },
  sNotes: {
    label: "Notes",
    tooltip: "Show the speaker notes under the slide",
    shortcut: "Ctrl Alt Shift S",
    whenToUse: "To write what you will say. Only you see them, in Presenter View and in notes pages.",
    fileEffect: UNTIL_SAVED,
    meaning: "Speaker notes are reminders for the person presenting; the audience never sees them.",
  },
  sFind: {
    label: "Find",
    tooltip: "Find and replace words on every slide, in tables and in notes",
    shortcut: "Ctrl F",
    whenToUse: "To find where a word is, or to change a name everywhere (Ctrl H opens Replace).",
    fileEffect: "Finding changes nothing. Replacing is one Undo step each time. " + UNTIL_SAVED,
  },
  sPlay: {
    label: "Play",
    tooltip: "Start the slide show from the first slide",
    shortcut: "F5",
    whenToUse: "To present. → or Space goes on, ← goes back, B blacks the screen, Esc ends. Shift F5 starts from this slide.",
    fileEffect: NOTHING_CHANGES,
  },
  sPresenterView: {
    label: "Presenter View",
    tooltip: "Present with your notes, the next slide and a timer",
    shortcut: "Alt F5",
    whenToUse: "With a projector: the audience sees the slides while your screen shows the notes and what comes next.",
    fileEffect: NOTHING_CHANGES,
    meaning: "With one screen, Presenter View shows in this window; with a second screen, the slides fill that one.",
  },
  sFrame: {
    label: "Move, Resize and Turn",
    tooltip: "Drag an object, its handles, or the round handle above it",
    whenToUse:
      "Drag to move; drag a square handle to resize (Shift keeps the proportions); drag the round handle to turn in 15° steps (Shift for any angle). Pink lines show what it lines up with. The arrows nudge a point at a time, ten with Shift. Double-click text to type.",
    fileEffect: UNTIL_SAVED,
  },
  sKept: {
    label: "Kept As It Is",
    tooltip: "Parts Letters keeps but can’t draw or edit yet",
    whenToUse:
      "Charts, SmartArt, embedded objects, shadows and glows, arrowheads and picture recolouring are kept exactly as they are in the file. Letters shows a box (or the picture saved with it) instead; PowerPoint and LibreOffice still show them. Animations and transitions are kept but don’t play here.",
    fileEffect: "Saving writes them back exactly as they were.",
  },
} satisfies Record<string, ToolHelp>;

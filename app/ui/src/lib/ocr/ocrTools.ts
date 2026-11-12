// the words of the Scan tab's tools and New from Images: label, tooltip,
// shortcut, when to use it and exactly what it does to the file. tools.ts
// adds them to the app's one list, so Explain shows them.

import type { ToolHelp } from "../tools";
import { OCR_MEANING } from "./ocrSentences";

const ONLY_THE_WORKING_COPY = "The file on disk changes only when you choose Save.";

export const ocrTools = {
  recognizeText: {
    label: "Recognize Text",
    tooltip: "Make scanned pages searchable by reading their words",
    shortcut: "Ctrl Alt O",
    whenToUse:
      "For a scanned reading, a photo of a handout or any page whose words can’t be selected. Afterwards Find, selecting, copying and redacting by text work on it.",
    fileEffect: `Adds an invisible layer of the words it read over each page picture in your working copy. The pages look exactly the same. Everything runs on this computer; nothing is sent anywhere. ${ONLY_THE_WORKING_COPY}`,
    meaning: OCR_MEANING,
  },
  showRecognizedText: {
    label: "Show Recognized Text",
    tooltip: "See the words the computer read, over the scan",
    whenToUse: "After Recognize Text, to check what was read. Doubtful words are tinted; click one to correct it.",
    fileEffect: `Nothing by itself. Correcting a word rewrites it in the invisible layer of your working copy, one Undo each. ${ONLY_THE_WORKING_COPY}`,
  },
  removeRecognizedText: {
    label: "Remove Recognized Text",
    tooltip: "Take off the text layer Letters added to scanned pages",
    whenToUse: "When the recognized text is poor and you want to read the pages again in another language, or not at all.",
    fileEffect: `Takes the invisible text off those pages of your working copy. The pictures stay. ${ONLY_THE_WORKING_COPY}`,
  },
  newFromImages: {
    label: "New from Images…",
    tooltip: "Make a PDF from photos or scans",
    whenToUse: "To turn phone photos of notes, or scanned pictures, into one PDF to read, search or hand in.",
    fileEffect: "Makes a new PDF from the pictures and opens it, unsaved. The pictures don’t change. Nothing is written until you choose Save As.",
  },
} satisfies Record<string, ToolHelp>;

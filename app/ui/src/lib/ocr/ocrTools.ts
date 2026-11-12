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
  makeEditableText: {
    label: "Make Editable Text",
    tooltip: "Turn scanned pages into real text you can change in the Edit tab",
    whenToUse:
      "When a scanned letter or form needs a word changed, not just found. Afterwards Edit Text and Images works on it like on any other page.",
    fileEffect: `Reads the pages if they haven’t been read, then writes the words as visible text in a standard font that closely matches the scan, at each word’s place. Text Over the Scan keeps the scan with its old letters painted out; Text Only keeps just the pictures, signatures and stamps. Everything runs on this computer. One Undo brings the scans back. ${ONLY_THE_WORKING_COPY}`,
    meaning: "Editable text is real text, like a typed document's; the scan was only a picture of words.",
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

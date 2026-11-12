// the words of the Protect tools: redaction, hidden information and
// passwords. they join the app's one list in ../tools.ts, so Explain,
// tooltips and the inspector read them like every other tool's.
//
// the file effects must stay exact. marking changes nothing; applying
// changes the working copy, and saving makes it permanent; a password is
// applied when the file is saved.

import type { ToolHelp } from "../tools";
import { REDACT_MEANING, UNDO_UNTIL_SAVED } from "./protectEffects";

const ONLY_WHEN_SAVED = "The file on disk changes only when you choose Save.";

export const protectTools = {
  redactMarkText: {
    label: "Mark Text",
    tooltip: "Mark selected text to be redacted",
    shortcut: "Ctrl Alt R",
    whenToUse:
      "For a name, a number or a sentence that must not be shared. Select the words, or click Mark Text and drag across them. Each line becomes a marked area.",
    fileEffect: "Nothing yet. Marks are only shown on screen until you apply them, and closing the document discards them.",
    meaning: REDACT_MEANING,
  },
  redactMarkArea: {
    label: "Mark Area",
    tooltip: "Draw boxes over what should be redacted",
    shortcut: "Ctrl Alt B",
    whenToUse:
      "For a picture, a signature, a stamp or text on a scanned page. From the keyboard, a box appears in the middle of the page; the arrows move it and Alt with the arrows changes its size.",
    fileEffect: "Nothing yet. Marks are only shown on screen until you apply them, and closing the document discards them.",
  },
  redactFind: {
    label: "Find and Mark…",
    tooltip: "Find a phrase, email addresses, phone numbers or long numbers to mark",
    whenToUse: "When the same name or number appears many times, or to catch every email address and ID number at once.",
    fileEffect: "Nothing yet. The matches you tick become marks, which change nothing until you apply them.",
  },
  redactReview: {
    label: "Review Redactions…",
    tooltip: "See exactly what the marked areas will remove",
    whenToUse: "Always, before applying: check the text that will go, the pictures and drawings that will be removed, and where the same words still appear.",
    fileEffect: "Nothing. The review redacts a temporary copy to show the result; your working copy and the file don’t change.",
  },
  redactApply: {
    label: "Apply Redactions…",
    tooltip: "Remove everything under the marked areas for good",
    whenToUse: "When the review shows exactly what you want removed.",
    fileEffect: `Removes the text, pictures, drawings, comments and form answers under the marks from your working copy and draws the boxes. ${UNDO_UNTIL_SAVED} Saving writes the whole file from scratch, so no old copy is left inside it.`,
    unavailable: "Mark text or areas first",
    meaning: REDACT_MEANING,
  },
  removeHiddenInfo: {
    label: "Remove Hidden Information…",
    tooltip: "Remove the author, metadata, attachments, scripts and other information the page doesn’t show",
    whenToUse: "Before sharing a file outside your class or school, and after redacting, since the author and title may still name someone.",
    fileEffect: `Removes what you tick from your working copy. Saving writes the whole file from scratch and deletes the earlier versions kept for Undo. ${ONLY_WHEN_SAVED}`,
    meaning: "Hidden information is what a PDF carries but doesn’t show on its pages, such as who wrote it and with which app.",
  },
  unlockForEditing: {
    label: "Unlock for Editing",
    tooltip: "Use the password so this locked PDF can be changed",
    whenToUse: "When a PDF you have the password for needs changes. Saving locks it again the way it was, unless you change that below.",
    fileEffect:
      "Nothing on disk. Letters keeps an unlocked working copy in its cache while the document is open and deletes it when you close the document.",
  },
  openPassword: {
    label: "Require a Password to Open",
    tooltip: "Encrypt the file so it opens only with a password",
    whenToUse: "For a file with private information that you send by email or keep on a shared computer.",
    fileEffect: `Encrypts the saved file with AES-256. Without the password nobody can read it, and Letters can’t recover a forgotten one. ${ONLY_WHEN_SAVED}`,
    meaning: "Encrypting scrambles the whole file so that only someone with the password can read it.",
  },
  restrictPermissions: {
    label: "Restrict What Others Can Do",
    tooltip: "Ask other apps not to allow printing, copying or changes",
    whenToUse: "To discourage casual copying or changes, for example of a handout. It is not a way to keep a secret.",
    fileEffect: `Writes the restrictions into the saved file. They are a request: anyone can still open the file, and some apps ignore them. ${ONLY_WHEN_SAVED}`,
  },
  removePassword: {
    label: "Remove Password",
    tooltip: "Save the file without a password or restrictions",
    whenToUse: "When a file no longer needs protecting, for example once it has been handed in.",
    fileEffect: `The saved file opens for anyone and has no restrictions. ${ONLY_WHEN_SAVED}`,
  },
} satisfies Record<string, ToolHelp>;

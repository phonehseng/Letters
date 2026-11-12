// the plain-english description of every Fill & Sign tool: label,
// tooltip, keys, when to use it, what it does to the file, and the
// meaning of words a first-year may not know. tools.ts lists these with
// every other tool, so Explain and the tooltips read them from one place.

import type { ToolHelp } from "../tools";

const ONLY_THE_WORKING_COPY = "The file on disk changes only when you choose Save.";

export const fillSignTools = {
  fillForm: {
    label: "Fill Form",
    tooltip: "Type into the form’s fields right on the page",
    whenToUse:
      "For a form a professor or an office sent you. Click a field, or press Tab, and type. Tab and Shift Tab move through the fields in order, across pages.",
    fileEffect: `Each answer is stored in the form fields of your working copy when you leave the field, as one undo step. ${ONLY_THE_WORKING_COPY}`,
    meaning: "A form field is a box in the PDF that takes an answer, such as a name, a tick or a choice from a list.",
  },
  showFields: {
    label: "Show Fields",
    tooltip: "Show the form’s fields as tinted boxes on the page",
    whenToUse: "To see where answers go. Turn it off to read the page without the tint.",
    fileEffect: "Nothing. Only the screen changes.",
  },
  nextEmptyField: {
    label: "Next Empty Field",
    tooltip: "Go to the next field that has no answer",
    shortcut: "Ctrl Alt E",
    whenToUse: "To finish a long form without missing anything, such as a required field on another page.",
    fileEffect: "Nothing. It only moves to the field.",
  },
  findFields: {
    label: "Find Fields…",
    tooltip: "Find the places to write on a form that has no fields",
    whenToUse:
      "When a form is only lines and boxes, so there is nothing to click. Letters suggests fields from the lines, boxes and labels it sees; you choose which to keep.",
    fileEffect: `Nothing until you choose Add Fields, which adds the fields you accepted to your working copy. ${ONLY_THE_WORKING_COPY}`,
    meaning:
      "Finding fields is a suggestion: Letters reads the lines and boxes drawn on the page. It can miss places, and it finds nothing on a scanned page (that needs text recognition, which comes later).",
  },
  addField: {
    label: "Add Field",
    tooltip: "Draw a new text box, checkbox, choice, date or signature field",
    whenToUse: "To make a form of your own, or to add a field that Find Fields missed. Choose the kind, then drag on the page.",
    fileEffect: `Adds a real form field to your working copy that other PDF apps can fill too. ${ONLY_THE_WORKING_COPY}`,
  },
  editFields: {
    label: "Edit Fields",
    tooltip: "Select fields to move, resize, rename or delete them",
    whenToUse: "To tidy a form: line fields up, give them clear names and help text, make some required.",
    fileEffect: `Changes the fields in your working copy. Undo takes each change back. ${ONLY_THE_WORKING_COPY}`,
  },
  flattenForm: {
    label: "Flatten Form…",
    tooltip: "Make the answers a permanent part of the pages",
    whenToUse: "Before sending a finished form, so the answers look the same everywhere and nobody can change them.",
    fileEffect: `Paints the answers into the pages of your working copy and removes the fields. Signature fields are kept unless you choose otherwise. Undo still works until you close the document. ${ONLY_THE_WORKING_COPY}`,
    // the same idea as flatten() in core/forms: "the answers part of the page itself"
    meaning:
      "Flattening makes the answers part of the page itself, so they look the same everywhere but can no longer be changed as form fields.",
  },
  signWithPicture: {
    label: "Sign with a Picture",
    tooltip: "Draw, type or choose a picture of your signature and put it on the page",
    whenToUse: "When a form only needs to look signed, such as a permission slip. Your signatures are kept on this computer for next time.",
    fileEffect: `Adds the picture to your working copy, where it can still be moved and resized. Kept signatures stay on this computer and are never put in a document until you place one. ${ONLY_THE_WORKING_COPY}`,
    meaning: "A picture of your signature; it does not prove who signed.",
  },
  signWithCertificate: {
    label: "Sign with a Certificate",
    tooltip: "Add a digital signature that others can check",
    whenToUse:
      "When a document must be signed in a way that can be checked, such as a contract. You need a certificate file (.p12 or .pfx) and its password, or you can make one here.",
    fileEffect:
      "Signs the saved file and saves it again at once, adding the signature at the end of the file. The password is used for this signature only and is never kept. Changes afterwards are saved after the signature, which still checks out and lists them, so Letters asks first. A document with a password to open, or restrictions, can’t be signed yet.",
    meaning: "A digital signature that lets others check the file has not changed since you signed.",
  },
  selfSignedCertificate: {
    label: "Create a Self-Signed Certificate…",
    tooltip: "Make your own certificate file for signing",
    whenToUse: "When you have no certificate from your school or employer. It proves the file hasn’t changed, but not who you are.",
    fileEffect: "Saves a new .p12 file, protected by the password you choose, where you say. No document changes.",
    meaning:
      "Self-signed means you made the certificate yourself, so nobody vouches for your name: other people’s apps show the signature as not verified.",
  },
  signatures: {
    label: "Signatures",
    tooltip: "Check who signed this document and whether it has changed since",
    whenToUse: "When you receive a signed document, or after signing one, to see that each signature is still valid.",
    fileEffect: "Nothing. Checking a signature only reads the file.",
    meaning:
      "Valid means the document hasn’t changed since it was signed. Letters doesn’t check trust lists, so it can’t confirm who the signer really is.",
  },
} satisfies Record<string, ToolHelp>;

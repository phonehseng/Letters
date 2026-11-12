// the sentences of the Protect tools: the "what will happen" line under
// each button, the apply sheet, and the lock line at the top of the tab.
//
// this is the part of the app where a promise that is too big hurts
// someone, so every sentence says exactly what the engine does and no
// more: redaction removes what is under the marks and nothing else, an
// open password encrypts, restrictions only ask.

import { describePages } from "../pageRanges";
import { areaCount, markedPages, type PendingMark } from "./pendingMarks";
import { restrictionSentence } from "./permissions";
import type { ProtectionStatus } from "./protectApi";

export const REDACT_MEANING = "Redacting removes text and pictures from the file for good, so nobody can copy or uncover them.";

export const UNDO_UNTIL_SAVED =
  "Undo works until you save or close the document. Saving deletes the earlier versions Letters kept for Undo, so the removed content is gone from this computer too. After you save and share the file, it is gone for good.";

export function markTextEffect(active: boolean, selectedPages: number[] | null, pageCount: number): string {
  if (selectedPages && selectedPages.length > 0) {
    return `Marks the selected text on ${describePages(selectedPages, pageCount)} for redaction. Nothing is removed until you apply.`;
  }
  if (active) return "Drag across text to mark it, one area per line. Shift and the arrow keys select from the keyboard; then press Ctrl Alt R. Escape stops.";
  return "Select text first, or click Mark Text and then drag across words. Nothing is removed until you apply.";
}

export function markAreaEffect(active: boolean): string {
  if (active) return "Drag a box over anything on the page: text, pictures or drawings. Escape stops.";
  return "Draws boxes over what should go, including pictures and scanned text. Nothing is removed until you apply.";
}

export function findEffect(chosen: number, total: number): string {
  if (total === 0) return "Finds every place a phrase or a kind of text appears, so you can mark them all at once.";
  if (chosen === 0) return "Tick the matches to mark.";
  return `Marks ${chosen === total && total > 1 ? `all ${total} matches` : chosen === 1 ? "1 match" : `${chosen} matches`} for redaction. Nothing is removed until you apply.`;
}

export function reviewEffect(marks: PendingMark[], pageCount: number): string {
  if (marks.length === 0) return "Mark text or areas first.";
  return `Shows exactly what ${marks.length === 1 ? "the area" : `the ${marks.length} areas`} on ${describePages(markedPages(marks), pageCount)} will remove. Nothing changes.`;
}

export function applyEffect(marks: PendingMark[], pageCount: number): string {
  if (marks.length === 0) return "Mark text or areas first.";
  return `Removes what is under ${areaCount(marks.length)} on ${describePages(markedPages(marks), pageCount)} from your working copy. It becomes permanent when you save.`;
}

/** the words of the apply sheet, in the manner of a mac's confirmation. */
export function applySheetText(marks: PendingMark[], pageCount: number): { title: string; message: string; notCovered: string[] } {
  const count = marks.length;
  const title = count === 1 ? "Apply 1 redaction?" : `Apply ${count} redactions?`;
  const message = `Everything under the ${count === 1 ? "marked area" : "marked areas"} on ${describePages(markedPages(marks), pageCount)} is removed from the document: text, pictures, drawings, comments and form answers. Once you save, it can’t be brought back. Undo works until you save or close the document.`;
  const notCovered = [
    "The same words anywhere you didn’t mark: other pages, the title, bookmarks or comments.",
    "Author and other hidden information. Use Remove Hidden Information afterwards.",
    "Fonts, which can show which letters were used, though not the words.",
  ];
  return { title, message, notCovered };
}

/** what the person hears once redactions were applied. */
export function appliedMessage(count: number): string {
  return `${count === 1 ? "1 area was" : `${count} areas were`} redacted. Save the document to make it final.`;
}

export function scrubEffect(chosen: number): string {
  if (chosen === 0) return "Tick what to remove.";
  return `Removes ${chosen === 1 ? "the ticked kind" : `the ${chosen} ticked kinds`} of hidden information from your working copy. The file on disk changes when you save.`;
}

export const OPEN_PASSWORD_HONESTY =
  "The open password really encrypts the file with AES-256: without it, nobody can read it, and Letters can’t recover a forgotten password.";

export const RESTRICTIONS_HONESTY =
  "Restrictions are a request: anyone can still open the file, and some apps ignore them. To keep people out, require a password to open.";

export function openPasswordEffect(ready: boolean, replacing: boolean): string {
  if (!ready) return "Type the password twice.";
  return `${replacing ? "Changes" : "Adds"} the password when you save. Until then, the file on disk opens as before.`;
}

export function restrictEffect(sentence: string, restricted: boolean): string {
  if (!restricted) return "Choose what to restrict.";
  return `${sentence} It takes effect when you save.`;
}

/** the line beside the lock at the top of the tab, or null when there is nothing to say. */
export function lockLine(status: ProtectionStatus | null): string | null {
  if (!status) return null;
  const method = status.lock?.method ?? "a password";
  if (status.locked) {
    return status.lock?.needsOpenPassword
      ? `Locked with ${method}. Read only until you unlock it for editing.`
      : `Has restrictions set by its author (${method}). Read only until you unlock it for editing.`;
  }
  if (status.undecided) return "Unlocked with the owner password. Choose whether it keeps a password to open before saving.";
  if (status.openPasswordOnSave) return "Locked with AES-256 when you save.";
  if (status.restrictionsOnSave) return `${restrictionSentence(status.restrictionsOnSave)} Takes effect when you save.`;
  if (status.changed && status.lock) return "Saved without a password from now on.";
  return null;
}

/** what saving does after a redaction or a scrub, for the line under the list. */
export function cleanSaveLine(status: ProtectionStatus | null): string | null {
  if (!status?.removedContent) return null;
  return "Saving writes the whole file from scratch and deletes the earlier versions kept for Undo.";
}

export function unlockEffect(status: ProtectionStatus | null): string {
  if (!status?.lock) return "";
  if (status.lock.needsOpenPassword) return "Enter its password to change it. The unlocked working copy is deleted when you close the document.";
  if (status.lock.permissions.editing) return "No password is needed: its author allowed changes. Saving keeps its other restrictions.";
  return "Its author asked that it not be changed. Enter the owner password, the one that set that limit.";
}

export function removePasswordEffect(status: ProtectionStatus | null): string {
  if (!status) return "";
  if (!status.openPasswordOnSave && !status.restrictionsOnSave && !status.undecided) return "This file has no password or restrictions to remove.";
  return "Saves the file without a password or restrictions. Anyone who has the file can then open and change it.";
}

/** true when there is a password or a restriction that Remove Password would take away. */
export function hasProtection(status: ProtectionStatus | null): boolean {
  return Boolean(status && (status.openPasswordOnSave || status.restrictionsOnSave || status.undecided));
}

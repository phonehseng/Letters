// the "what will happen" line under each Fill & Sign button, worked out
// from the settings, so it always says exactly what the button does.

import type { Proposal } from "./proposals";
import { counts } from "./proposals";

const ONLY_THE_WORKING_COPY = "The file on disk changes only when you choose Save.";

function plural(count: number, one: string, many = `${one}s`): string {
  return `${count} ${count === 1 ? one : many}`;
}

export function fillEffect(fieldCount: number, emptyCount: number): string {
  if (fieldCount === 0) return "This document has no fields to fill. Find Fields… or Add Field can add some.";
  const goes = emptyCount > 0 ? `Goes to the next of ${plural(emptyCount, "field")} without an answer.` : "Every field has an answer.";
  return `${goes} Each answer is stored in your working copy when you leave its field. ${ONLY_THE_WORKING_COPY}`;
}

export function findEffect(list: Proposal[], scope: "page" | "all", page: number, pageCount: number): string {
  if (list.length === 0) {
    const where = scope === "page" ? `page ${page + 1}` : `all ${plural(pageCount, "page")}`;
    return `Looks for lines, boxes and labels on ${where}. Nothing changes until you choose Add Fields.`;
  }
  const tally = counts(list);
  if (tally.accepted === 0) return "Accept the suggestions to keep, on the page or in the list, then choose Add Fields.";
  const left = tally.pending + tally.rejected;
  const leftOut = left > 0 ? ` ${plural(left, "suggestion")} not accepted ${left === 1 ? "is" : "are"} left out.` : "";
  return `Adds ${plural(tally.accepted, "field")} to your working copy, as one undo step.${leftOut} ${ONLY_THE_WORKING_COPY}`;
}

export function addFieldEffect(kindLabel: string, drawing: boolean): string {
  if (drawing) return "Drag on the page where the field goes, or click for the usual size. Escape stops.";
  return `Adds a ${kindLabel.toLowerCase()} field to your working copy. ${ONLY_THE_WORKING_COPY}`;
}

export function flattenEffect(fieldCount: number, keepSignatures: boolean, signatureFields: number): string {
  if (fieldCount === 0) return "There are no form fields to flatten.";
  const kept = keepSignatures && signatureFields > 0 ? ` ${plural(signatureFields, "signature field")} ${signatureFields === 1 ? "stays" : "stay"} a field.` : "";
  return `Paints the answers of ${plural(fieldCount, "field")} into the pages of your working copy. They can’t be changed afterwards, but Undo still works until you close the document.${kept} ${ONLY_THE_WORKING_COPY}`;
}

export function placeSignatureEffect(hasPicture: boolean, placing: boolean): string {
  if (!hasPicture) return "Draw, type or choose your signature, then choose Keep Signature.";
  if (placing) return "Click the page or an empty signature field to place it, or drag to choose its size. Escape stops.";
  return `Puts the picture on the page in your working copy. Click it with Select later to move or resize it. ${ONLY_THE_WORKING_COPY}`;
}

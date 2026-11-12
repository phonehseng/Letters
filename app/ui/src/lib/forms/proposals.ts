// the fields Find Fields proposes on a flat form, and what the person
// decides about each one before Add Fields makes them real.
//
// detection is only a suggestion: every proposal starts undecided, the
// person accepts all or goes through them (accept, reject, move, resize,
// change the kind), and only accepted ones become fields. nothing here
// touches the screen or the document.

import type { Rect } from "../pageTools/pagePoints";
import type { DetectedKind, NewField, PageProposals } from "./formsApi";

export type ProposalStatus = "pending" | "accepted" | "rejected";

export interface Proposal {
  id: string;
  page: number;
  kind: DetectedKind;
  rect: Rect;
  /** the words beside it that probably name it ("Name:") */
  label: string | null;
  name: string;
  /** for combs: how many boxes */
  maxLength: number | null;
  /** radio buttons in one row or column share a group (per page) */
  group: number | null;
  confidence: number;
  status: ProposalStatus;
}

export const KIND_LABELS: Record<DetectedKind, string> = {
  text: "Text",
  multilineText: "Text Box",
  checkbox: "Checkbox",
  radio: "Choice",
  comb: "Boxes",
};

/** the engine's proposals for some pages, all undecided. */
export function proposalsFrom(pages: PageProposals[]): Proposal[] {
  return pages.flatMap(({ page, fields }) =>
    fields.map((field, index) => ({
      id: `${page}-${index}`,
      page,
      kind: field.kind,
      rect: field.rect,
      label: field.label,
      name: field.suggestedName,
      maxLength: field.maxLength,
      group: field.group,
      confidence: field.confidence,
      status: "pending" as const,
    })),
  );
}

function change(list: Proposal[], id: string, update: (proposal: Proposal) => Proposal): Proposal[] {
  return list.map((proposal) => (proposal.id === id ? update(proposal) : proposal));
}

/** every proposal not yet rejected is accepted. */
export function acceptAll(list: Proposal[]): Proposal[] {
  return list.map((proposal) => (proposal.status === "rejected" ? proposal : { ...proposal, status: "accepted" }));
}

export function setStatus(list: Proposal[], id: string, status: ProposalStatus): Proposal[] {
  return change(list, id, (proposal) => ({ ...proposal, status }));
}

/** moving or resizing a proposal also accepts it: the person clearly wants it. */
export function moveProposal(list: Proposal[], id: string, rect: Rect): Proposal[] {
  return change(list, id, (proposal) => ({ ...proposal, rect, status: proposal.status === "rejected" ? "rejected" : "accepted" }));
}

/** a comb needs a number of boxes: square boxes across its width when it had none. */
export function setKind(list: Proposal[], id: string, kind: DetectedKind): Proposal[] {
  return change(list, id, (proposal) => {
    const boxes = proposal.maxLength ?? Math.max(1, Math.round(proposal.rect.width / Math.max(1, proposal.rect.height)));
    return { ...proposal, kind, maxLength: kind === "comb" ? boxes : proposal.maxLength };
  });
}

export function counts(list: Proposal[]): Record<ProposalStatus, number> {
  const tally = { pending: 0, accepted: 0, rejected: 0 };
  for (const proposal of list) tally[proposal.status] += 1;
  return tally;
}

/** what a proposal is called in lists: its label, or its kind. */
export function proposalLabel(proposal: Proposal): string {
  return proposal.label?.replace(/[:\s]+$/, "") || KIND_LABELS[proposal.kind];
}

/**
 * the fields to add for the accepted proposals. radio buttons of one
 * group become one radio group whose buttons are "Choice 1", "Choice 2"
 * (or their labels); a comb becomes a text field with one box per letter.
 */
export function newFieldsFrom(list: Proposal[]): NewField[] {
  const accepted = list.filter((proposal) => proposal.status === "accepted");
  const fields: NewField[] = [];
  const groups = new Map<string, Proposal[]>();
  for (const proposal of accepted) {
    if (proposal.kind === "radio") {
      // a radio button without a group is a group of its own
      const key = proposal.group === null ? proposal.id : `${proposal.page}:${proposal.group}`;
      groups.set(key, [...(groups.get(key) ?? []), proposal]);
      continue;
    }
    fields.push(singleField(proposal));
  }
  for (const buttons of groups.values()) fields.push(radioGroup(buttons));
  return fields;
}

function singleField(proposal: Proposal): NewField {
  const base = { name: proposal.name, page: proposal.page, rect: proposal.rect, tooltip: proposal.label ? proposalLabel(proposal) : null };
  switch (proposal.kind) {
    case "multilineText":
      return { ...base, kind: { type: "multilineText" } };
    case "checkbox":
      return { ...base, kind: { type: "checkbox" } };
    case "comb":
      return { ...base, kind: { type: "text", maxLength: proposal.maxLength ?? 1, comb: true } };
    default:
      return { ...base, kind: { type: "text" } };
  }
}

function radioGroup(buttons: Proposal[]): NewField {
  const first = buttons[0];
  const used = new Set<string>();
  const values = buttons.map((button, index) => {
    let value = button.label ? proposalLabel(button) : `Choice ${index + 1}`;
    // two buttons can't stand for the same answer
    if (used.has(value)) value = `${value} ${index + 1}`;
    used.add(value);
    return { value, rect: button.rect };
  });
  return { name: first.name, page: first.page, rect: first.rect, kind: { type: "radioGroup", buttons: values }, tooltip: null };
}

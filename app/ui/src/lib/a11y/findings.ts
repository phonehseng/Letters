// a check's findings as the Accessibility tab shows them: grouped by how
// urgent they are, with where each is said in words, and a key that
// recognises the same finding after the document is checked again.

import type { Finding, FixKind, Report, Severity } from "./a11yApi";

export const GROUPS: { severity: Severity; title: string; hint: string }[] = [
  {
    severity: "mustFix",
    title: "Must Fix",
    hint: "These stop screen readers from using the document, or break PDF/UA.",
  },
  {
    severity: "shouldFix",
    title: "Should Fix",
    hint: "These make the document harder to use. They are worth fixing.",
  },
  {
    severity: "checkByHand",
    title: "Check by Hand",
    hint: "No program can decide these. Look at each one yourself.",
  },
];

export interface FindingGroup {
  severity: Severity;
  title: string;
  hint: string;
  findings: Finding[];
}

/** the findings in their three groups, empty groups left out. */
export function groupFindings(findings: Finding[]): FindingGroup[] {
  return GROUPS.map((group) => ({ ...group, findings: findings.filter((finding) => finding.severity === group.severity) })).filter(
    (group) => group.findings.length > 0,
  );
}

/** "Page 3", "Page 3, in Document › Figure 2", or "The whole document". */
export function whereText(finding: Finding): string {
  const { page, structurePath } = finding.where;
  const pagePart = page === null ? null : `Page ${page + 1}`;
  if (pagePart && structurePath) return `${pagePart}, in ${structurePath}`;
  if (pagePart) return pagePart;
  if (structurePath) return structurePath;
  return "The whole document";
}

/** true when the finding has a place on a page to jump to. */
export function hasSpot(finding: Finding): boolean {
  return finding.where.page !== null;
}

/**
 * a key that stays the same for one problem across checks: the rule and
 * where it is. object numbers stay the same across edits (lopdf keeps
 * them), so an element or annotation identifies the place well.
 */
export function findingKey(finding: Finding): string {
  const { page, rect, element, annotation } = finding.where;
  const object = element ?? annotation;
  const place = object
    ? `${object.number}.${object.generation}`
    : rect
      ? `${page}:${Math.round(rect.x)},${Math.round(rect.y)}`
      : `${page ?? "doc"}`;
  return `${finding.id}@${place}`;
}

/** the fixes that run with no answer from the person: one button is enough. */
const AUTOMATIC: FixKind["kind"][] = [
  "markAsTagged",
  "showTitle",
  "setTabOrder",
  "tagAnnotations",
  "tagUntaggedContent",
  "addBookmarksFromHeadings",
];

export function isAutomatic(fix: FixKind): boolean {
  return AUTOMATIC.includes(fix.kind);
}

/** fixes the wizard can carry out here (text recognition belongs to another tool). */
export function isFixableHere(finding: Finding): finding is Finding & { fix: FixKind } {
  return finding.fix !== null && finding.fix.kind !== "recogniseText";
}

/** the findings the wizard walks through, most urgent first, in page order within a group. */
export function wizardFindings(report: Report): Finding[] {
  const order: Severity[] = ["mustFix", "shouldFix", "checkByHand"];
  const fixable = report.findings.filter(isFixableHere);
  // one heading ladder and one "add tags" fix cover every finding of their rule
  const once = new Set<string>();
  const unique = fixable.filter((finding) => {
    const kind = finding.fix!.kind;
    const coversAll = kind === "fixHeadingLevels" || kind === "addTags" || kind === "tagAnnotations" || kind === "setTabOrder";
    if (!coversAll) return true;
    if (once.has(kind)) return false;
    once.add(kind);
    return true;
  });
  return unique
    .map((finding, index) => ({ finding, index }))
    .sort((a, b) => {
      const bySeverity = order.indexOf(a.finding.severity) - order.indexOf(b.finding.severity);
      if (bySeverity !== 0) return bySeverity;
      // adding tags comes first: most other fixes need tags
      const tagsFirst = Number(b.finding.fix?.kind === "addTags") - Number(a.finding.fix?.kind === "addTags");
      if (tagsFirst !== 0) return tagsFirst;
      return a.index - b.index;
    })
    .map((entry) => entry.finding);
}

/** the counts line under the score: "3 must fix, 2 should fix, 6 to check by hand". */
export function countsLine(report: Report): string {
  const { mustFix, shouldFix, checkByHand } = report.summary;
  if (mustFix + shouldFix + checkByHand === 0) return "Nothing was found.";
  return `${mustFix} must fix, ${shouldFix} should fix, ${checkByHand} to check by hand`;
}

/** what a re-check found compared with before: the problems now gone. */
export function solvedSince(before: Report, after: Report): Finding[] {
  const still = new Set(after.findings.map(findingKey));
  return before.findings.filter((finding) => finding.severity !== "checkByHand" && !still.has(findingKey(finding)));
}

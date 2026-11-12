// which pages Recognize Text reads: the page being read, the pages
// selected in the sidebar, every page, or only the pages that are scans.

export type PageChoice = "current" | "selection" | "all" | "scanned";

export interface PageFacts {
  /** counted from 0 */
  current: number;
  selection: number[];
  pageCount: number;
  /** pages that are mostly a picture, from the scan overview */
  scanned: number[];
}

function uniqueSorted(pages: number[], pageCount: number): number[] {
  return [...new Set(pages)].filter((page) => page >= 0 && page < pageCount).sort((a, b) => a - b);
}

export function pagesFor(choice: PageChoice, facts: PageFacts): number[] {
  switch (choice) {
    case "current":
      return uniqueSorted([facts.current], facts.pageCount);
    case "selection":
      return uniqueSorted(facts.selection.length > 0 ? facts.selection : [facts.current], facts.pageCount);
    case "all":
      return Array.from({ length: facts.pageCount }, (_, index) => index);
    case "scanned":
      return uniqueSorted(facts.scanned, facts.pageCount);
  }
}

/** the choices the tab offers: a selection only when there is one, scanned pages only when some are. */
export function availableChoices(facts: PageFacts): { value: PageChoice; label: string }[] {
  const choices: { value: PageChoice; label: string }[] = [{ value: "current", label: "This Page" }];
  if (facts.selection.length > 0) choices.push({ value: "selection", label: "Selected" });
  choices.push({ value: "all", label: "All" });
  if (facts.scanned.length > 0 && facts.scanned.length < facts.pageCount) choices.push({ value: "scanned", label: "Scanned Only" });
  return choices;
}

/** the starting choice: the scanned pages when only some are scans, else every page. */
export function defaultChoice(facts: PageFacts): PageChoice {
  if (facts.scanned.length > 0 && facts.scanned.length < facts.pageCount) return "scanned";
  return facts.pageCount === 1 ? "current" : "all";
}

/** "page 3", "pages 1–4 and 7", "all 14 pages". */
export function pagesInWords(pages: number[], pageCount: number): string {
  if (pages.length === 0) return "no pages";
  if (pages.length === pageCount && pageCount > 1) return `all ${pageCount} pages`;
  const runs: string[] = [];
  let start = pages[0];
  let previous = pages[0];
  for (const page of [...pages.slice(1), Number.NaN]) {
    if (page === previous + 1) {
      previous = page;
      continue;
    }
    runs.push(start === previous ? `${start + 1}` : `${start + 1}–${previous + 1}`);
    start = page;
    previous = page;
  }
  const listed = runs.length === 1 ? runs[0] : `${runs.slice(0, -1).join(", ")} and ${runs[runs.length - 1]}`;
  return pages.length === 1 ? `page ${listed}` : `pages ${listed}`;
}

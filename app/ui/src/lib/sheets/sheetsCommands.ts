// the spreadsheet commands every window knows, joined into the app's one
// list in ../commands.ts, and the starter workbook Recents' New asked
// for, waiting for the window to open it.

export const SHEETS_COMMANDS = ["new-spreadsheet"] as const;

/** the starter workbooks of core/sheets (help::presets::TEMPLATES), with their names on Recents. */
export const STARTERS: { template: string | null; name: string; about: string }[] = [
  { template: null, name: "Blank", about: "An empty sheet" },
  { template: "budget", name: "Budget", about: "Income and spending with totals" },
  { template: "grades", name: "Grades", about: "Marks, averages and letter grades" },
  { template: "lab", name: "Lab Data", about: "Measurements with averages and spread" },
  { template: "schedule", name: "Schedule", about: "A week of classes and tasks" },
  { template: "invoice", name: "Invoice", about: "Items, prices and a total" },
];

let pending: string | null = null;

/** Recents' New: remembers the choice, the window's "new-spreadsheet" command opens it. */
export function chooseStarter(template: string | null) {
  pending = template;
}

export function takeStarter(): string | null {
  const template = pending;
  pending = null;
  return template;
}

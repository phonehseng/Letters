// the rows of the font picker: favourites, recent fonts, the document's
// fonts, then every font on this computer, each under a heading. with
// text typed, the engine's ranked matches replace the groups.

import type { FamilySummary } from "./fontsApi";

export type PickerRow =
  | { kind: "heading"; label: string }
  | { kind: "family"; name: string; family: FamilySummary | null; group: string };

const key = (name: string) => name.trim().replace(/\s+/g, " ").toLowerCase();

/** families by their key, for looking names up. */
export function familyMap(families: FamilySummary[]): Map<string, FamilySummary> {
  return new Map(families.map((family) => [key(family.name), family]));
}

function group(label: string, names: string[], byName: Map<string, FamilySummary>, seen?: Set<string>): PickerRow[] {
  const rows: PickerRow[] = [];
  for (const name of names) {
    const found = byName.get(key(name)) ?? null;
    if (seen?.has(key(name))) continue;
    seen?.add(key(name));
    rows.push({ kind: "family", name: found?.name ?? name, family: found, group: label });
  }
  return rows.length ? [{ kind: "heading", label }, ...rows] : [];
}

/**
 * the picker's rows with nothing typed. a font can be in Favourites and
 * in All Fonts, as in other apps' menus; document fonts that aren't on
 * this computer are listed too, so a missing one can still be seen.
 */
export function groupedRows(
  families: FamilySummary[],
  personal: { favourites: string[]; recent: string[] },
  documentFonts: string[],
): PickerRow[] {
  const byName = familyMap(families);
  const usable = (name: string) => byName.get(key(name))?.onThisComputer ?? false;
  const everything = families.filter((family) => family.onThisComputer).map((family) => family.name);
  return [
    ...group("Favourites", personal.favourites.filter(usable), byName),
    ...group("Recent", personal.recent.filter(usable).slice(0, 6), byName),
    ...group("In This Document", [...new Set(documentFonts.filter((name) => name.trim()))], byName),
    ...group("All Fonts", everything, byName),
  ];
}

/** the picker's rows for a search: the matches on this computer, best first. */
export function matchRows(families: FamilySummary[], positions: number[]): PickerRow[] {
  const rows: PickerRow[] = [];
  for (const position of positions) {
    const family = families[position];
    if (family?.onThisComputer) rows.push({ kind: "family", name: family.name, family, group: "Matches" });
  }
  return rows;
}

export function isFamilyRow(row: PickerRow | undefined): row is Extract<PickerRow, { kind: "family" }> {
  return row?.kind === "family";
}

/** the row of the chosen font, so the picker opens with it in view. */
export function indexOfFamily(rows: PickerRow[], name: string): number {
  return rows.findIndex((row) => row.kind === "family" && key(row.name) === key(name));
}

/** the subtle note under the picker when a font can't be embedded. */
export function embeddingNote(family: FamilySummary | null | undefined): string | null {
  if (!family || family.embeddable) return null;
  return "This font’s license doesn’t allow embedding; others will see a similar font unless they have it.";
}

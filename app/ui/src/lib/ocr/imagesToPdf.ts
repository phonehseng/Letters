// New from Images: which files are pictures, putting them in order, and
// the line that says what will be made.

import type { PaperSize } from "./ocrApi";

const PICTURE = /\.(png|jpe?g|tiff?)$/i;

export function isPicturePath(path: string): boolean {
  return PICTURE.test(path);
}

export function picturePathsAmong(paths: string[]): string[] {
  return paths.filter(isPicturePath);
}

/** the list with one item moved to another place. */
export function moveItem<T>(items: T[], from: number, to: number): T[] {
  if (from < 0 || from >= items.length || to < 0 || to >= items.length || from === to) return items;
  const moved = [...items];
  const [item] = moved.splice(from, 1);
  moved.splice(to, 0, item);
  return moved;
}

/** adds pictures to the list, leaving out ones already in it. */
export function addPictures(current: string[], added: string[]): string[] {
  const seen = new Set(current);
  return [...current, ...picturePathsAmong(added).filter((path) => !seen.has(path) && seen.add(path))];
}

export const PAPER_CHOICES: { value: PaperSize; label: string }[] = [
  { value: "fromImage", label: "Picture Size" },
  { value: "letter", label: "Letter" },
  { value: "a4", label: "A4" },
];

export function paperSentence(paper: PaperSize): string {
  if (paper === "fromImage") return "Each page is exactly the size of its picture, from the resolution stored in it. Right for scans.";
  const name = paper === "a4" ? "A4" : "US Letter";
  return `Each picture is fitted onto a ${name} page with a half-inch margin, turned sideways when it is wider than tall. Right for phone photos.`;
}

/** the line under Make PDF. */
export function makeSentence(count: number, readText: boolean, language: string): string {
  if (count === 0) return "Choose or drop PNG, JPEG or TIFF pictures first.";
  const pages = count === 1 ? "a PDF with 1 page" : `a PDF with ${count} pages, in this order`;
  const reading = readText ? `, reads their text in ${language} so it can be searched,` : "";
  return `Makes ${pages}${reading} and opens it here. Nothing is saved until you choose Save As.`;
}

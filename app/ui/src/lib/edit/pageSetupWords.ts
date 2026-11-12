// the words and numbers of Page Setup: paper sizes, what each way of
// fitting does, and the five page boxes explained for someone who has
// never heard of them. points are the pdf's unit: 72 to the inch.

import type { MarginFit, PageBoxes, PaperFit, Side } from "./contentApi";

export const POINTS_PER_INCH = 72;

export interface PaperSize {
  id: string;
  label: string;
  width: number;
  height: number;
}

// iso 216 and the north american sizes, in whole points as pdf writers round them
export const PAPER_SIZES: PaperSize[] = [
  { id: "letter", label: "Letter (8.5 × 11 in)", width: 612, height: 792 },
  { id: "a4", label: "A4 (210 × 297 mm)", width: 595, height: 842 },
  { id: "legal", label: "Legal (8.5 × 14 in)", width: 612, height: 1008 },
  { id: "a5", label: "A5 (148 × 210 mm)", width: 420, height: 595 },
  { id: "a3", label: "A3 (297 × 420 mm)", width: 842, height: 1191 },
];

/** the paper a page's size is, within a point either way, or null. */
export function paperNamed(width: number, height: number): PaperSize | null {
  const near = (a: number, b: number) => Math.abs(a - b) <= 1;
  return PAPER_SIZES.find((paper) => (near(paper.width, width) && near(paper.height, height)) || (near(paper.width, height) && near(paper.height, width))) ?? null;
}

/** "8.5 in" from points, for the hints beside number fields. */
export function inches(points: number): string {
  return `${Math.round((points / POINTS_PER_INCH) * 100) / 100} in`;
}

export const MARGIN_FITS: { value: MarginFit; label: string; sentence: string }[] = [
  { value: "scale", label: "Scale", sentence: "Makes the content smaller or larger, keeping its proportions, so it fits inside the new margins." },
  { value: "shift", label: "Move Only", sentence: "Keeps the content its size and moves it so its top left corner sits on the new margins." },
];

export const PAPER_FITS: { value: PaperFit; label: string; sentence: string }[] = [
  { value: "fit", label: "Fit", sentence: "Scales the page to fit the new paper, centred; nothing is cut off." },
  { value: "fill", label: "Fill", sentence: "Scales the page to cover the new paper, centred; what spills over is cut off." },
  { value: "keepSize", label: "Keep Size", sentence: "Keeps everything its size, centred on the new paper." },
];

export const SIDES: { value: Side; label: string }[] = [
  { value: "top", label: "Top" },
  { value: "bottom", label: "Bottom" },
  { value: "left", label: "Left" },
  { value: "right", label: "Right" },
];

/** what adding space does, for the line under it. */
export function spaceSentence(side: Side, points: number): string {
  if (points === 0) return "Type how many points to add (or a negative number to take away).";
  const way = points > 0 ? "Adds" : "Takes away";
  return `${way} ${Math.abs(points)} pt (${inches(Math.abs(points))}) of white space at the ${side}. The content stays where it is.`;
}

export type BoxName = keyof PageBoxes;

export const BOXES: { name: BoxName; label: string; meaning: string }[] = [
  { name: "media", label: "Paper (Media Box)", meaning: "The whole sheet the page is printed on. Every other box must fit inside it." },
  { name: "crop", label: "Shown (Crop Box)", meaning: "The part readers show on screen and print. Anything outside is hidden, not removed." },
  { name: "bleed", label: "Bleed Box", meaning: "For professional printing: colour that runs off the edge is printed up to here, then cut." },
  { name: "trim", label: "Finished Size (Trim Box)", meaning: "The size of the page after the printer cuts it." },
  { name: "art", label: "Content (Art Box)", meaning: "The part with the meaningful content, used when the page is placed in another document." },
];

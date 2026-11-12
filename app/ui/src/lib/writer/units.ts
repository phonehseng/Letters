// lengths: the model counts in twips (1/1440 inch), the screen in css
// pixels (96 to the inch), people in inches or centimetres.

export const TWIPS_PER_INCH = 1440;
export const TWIPS_PER_CM = 1440 / 2.54;
export const TWIPS_PER_POINT = 20;
export const CSS_PIXELS_PER_INCH = 96;

/** twips to css pixels at 100% zoom (15 twips make a pixel). */
export function twipsToPx(twips: number): number {
  return (twips * CSS_PIXELS_PER_INCH) / TWIPS_PER_INCH;
}

export function pxToTwips(px: number): number {
  return Math.round((px * TWIPS_PER_INCH) / CSS_PIXELS_PER_INCH);
}

export function twipsToPoints(twips: number): number {
  return twips / TWIPS_PER_POINT;
}

export function pointsToTwips(points: number): number {
  return Math.round(points * TWIPS_PER_POINT);
}

export type LengthUnit = "in" | "cm";

/**
 * inches in the few countries that still use them for paper, centimetres
 * everywhere else. the region comes from the system language tag
 * ("en-US", "en-GB"); a tag without a region counts as US english only
 * when it is plain "en".
 */
export function unitForLocale(locale: string): LengthUnit {
  const parts = locale.split(/[-_]/);
  const region = parts.slice(1).find((part) => /^[A-Za-z]{2}$/.test(part))?.toUpperCase();
  if (!region) return parts[0].toLowerCase() === "en" ? "in" : "cm";
  return ["US", "LR", "MM", "PR"].includes(region) ? "in" : "cm";
}

/** a length for people: "0.5 in", "1.27 cm". */
export function formatLength(twips: number, unit: LengthUnit): string {
  const value = unit === "in" ? twips / TWIPS_PER_INCH : twips / TWIPS_PER_CM;
  const rounded = Math.round(value * 100) / 100;
  return `${Object.is(rounded, -0) ? 0 : rounded} ${unit}`;
}

/** the number part only, for a field: 0.5 */
export function lengthValue(twips: number, unit: LengthUnit): number {
  const value = unit === "in" ? twips / TWIPS_PER_INCH : twips / TWIPS_PER_CM;
  return Math.round(value * 100) / 100;
}

export function lengthToTwips(value: number, unit: LengthUnit): number {
  return Math.round(unit === "in" ? value * TWIPS_PER_INCH : value * TWIPS_PER_CM);
}

/** the step arrow keys move a ruler marker by: an eighth of an inch, or a quarter centimetre. */
export function rulerStep(unit: LengthUnit, large = false): number {
  const step = unit === "in" ? TWIPS_PER_INCH / 8 : TWIPS_PER_CM / 4;
  return Math.round(large ? step * 4 : step);
}

// colour contrast as WCAG 2 measures it, for two jobs: showing the
// wizard's contrast step in words, and auditing the app's own colours
// (the tokens in app.css) in light, dark and increased-contrast modes.

export interface Colour {
  r: number;
  g: number;
  b: number;
  /** 0 (see-through) to 1 (solid) */
  a: number;
}

/**
 * reads the colours app.css uses: #rgb, #rrggbb, rgb(r g b), rgb(r g b / a),
 * and the older comma forms.
 */
export function parseColour(text: string): Colour | null {
  const value = text.trim().toLowerCase();
  const hex = /^#([0-9a-f]{3}|[0-9a-f]{6})$/.exec(value);
  if (hex) {
    const digits = hex[1].length === 3 ? [...hex[1]].map((d) => d + d).join("") : hex[1];
    return { r: parseInt(digits.slice(0, 2), 16), g: parseInt(digits.slice(2, 4), 16), b: parseInt(digits.slice(4, 6), 16), a: 1 };
  }
  const rgb = /^rgba?\(([^)]+)\)$/.exec(value);
  if (!rgb) return null;
  const parts = rgb[1].split(/[\s,/]+/).filter(Boolean).map(Number);
  if (parts.length < 3 || parts.some((part) => Number.isNaN(part))) return null;
  return { r: parts[0], g: parts[1], b: parts[2], a: parts[3] ?? 1 };
}

/** a see-through colour as it looks over a solid background. */
export function over(top: Colour, background: Colour): Colour {
  const mix = (front: number, back: number) => front * top.a + back * (1 - top.a);
  return { r: mix(top.r, background.r), g: mix(top.g, background.g), b: mix(top.b, background.b), a: 1 };
}

/** how bright a colour looks, from 0 (black) to 1 (white). */
export function luminance(colour: Colour): number {
  const channel = (value: number) => {
    const unit = value / 255;
    return unit <= 0.04045 ? unit / 12.92 : ((unit + 0.055) / 1.055) ** 2.4;
  };
  return 0.2126 * channel(colour.r) + 0.7152 * channel(colour.g) + 0.0722 * channel(colour.b);
}

/** the contrast ratio, from 1 (the same) to 21 (black on white). */
export function contrastRatio(foreground: Colour, background: Colour): number {
  const front = over(foreground, background);
  const [light, dark] = [luminance(front), luminance(background)].sort((a, b) => b - a);
  return (light + 0.05) / (dark + 0.05);
}

/** what WCAG asks of text: 4.5:1, or 3:1 for large text (18 pt, or 14 pt bold). */
export function requiredRatio(sizePt: number, bold: boolean): number {
  return sizePt >= 18 || (bold && sizePt >= 14) ? 3 : 4.5;
}

/** "2.3 to 1", the way the ratio is said. */
export function ratioText(ratio: number): string {
  return `${(Math.floor(ratio * 10) / 10).toFixed(1)} to 1`;
}

export function hex(colour: { r: number; g: number; b: number }): string {
  const two = (value: number) => Math.round(value).toString(16).padStart(2, "0");
  return `#${two(colour.r)}${two(colour.g)}${two(colour.b)}`;
}

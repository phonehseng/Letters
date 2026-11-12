// the app's own colours checked against WCAG 2: every pair of a text or
// control colour and the surface it sits on, in light and dark mode, each
// with and without increased contrast. the colours come straight from
// the design tokens in app.css, so the audit can't drift from the app.

import { contrastRatio, over, parseColour, type Colour } from "./contrast";

export type Mode = "light" | "dark" | "lightMoreContrast" | "darkMoreContrast";

export const MODES: Mode[] = ["light", "dark", "lightMoreContrast", "darkMoreContrast"];

/** the token blocks of a stylesheet: the base `:root`, and each media query's. */
export function tokenBlocks(css: string): { media: string | null; tokens: Record<string, string> }[] {
  const blocks: { media: string | null; tokens: Record<string, string> }[] = [];
  const pattern = /(@media[^{]*)?\{\s*:root\s*\{([^}]*)\}|(^|\n):root\s*\{([^}]*)\}/g;
  for (const match of css.matchAll(pattern)) {
    const media = match[1] ? match[1].replace("@media", "").trim() : null;
    const body = match[2] ?? match[4] ?? "";
    const tokens: Record<string, string> = {};
    for (const token of body.matchAll(/(--[a-z0-9-]+)\s*:\s*([^;]+);/g)) tokens[token[1]] = token[2].trim();
    blocks.push({ media, tokens });
  }
  return blocks;
}

/** which media queries apply in a mode. */
function applies(media: string, mode: Mode): boolean {
  const dark = mode === "dark" || mode === "darkMoreContrast";
  const more = mode === "lightMoreContrast" || mode === "darkMoreContrast";
  const conditions = [...media.matchAll(/\(([^)]+)\)/g)].map((match) => match[1].replace(/\s+/g, ""));
  return conditions.every((condition) => {
    if (condition === "prefers-color-scheme:dark") return dark;
    if (condition === "prefers-contrast:more") return more;
    // reduced transparency and the rest are not part of these four modes
    return false;
  });
}

/** every token's value in a mode, later blocks overriding earlier ones as css does. */
export function tokensFor(css: string, mode: Mode): Record<string, string> {
  const merged: Record<string, string> = {};
  for (const block of tokenBlocks(css)) {
    if (block.media === null || applies(block.media, mode)) Object.assign(merged, block.tokens);
  }
  return merged;
}

/** a pair to check: a foreground on a background, and what it must reach. */
export interface ContrastPair {
  name: string;
  foreground: string;
  background: string;
  /** 4.5 for text, 3 for parts of controls and large text */
  required: number;
  /** a see-through background is laid over these first, nearest first (the last must be solid) */
  under?: string | string[];
}

/** the pairs the app's screens use (the components say which token sits on which). */
export const PAIRS: ContrastPair[] = [
  { name: "text on the window", foreground: "--label", background: "--window", required: 4.5 },
  { name: "secondary text on the window", foreground: "--label-secondary", background: "--window", required: 4.5 },
  { name: "secondary text on glass", foreground: "--label-secondary", background: "--glass", under: "--desk", required: 4.5 },
  { name: "text on glass", foreground: "--label", background: "--glass", under: "--desk", required: 4.5 },
  { name: "accent text on the window", foreground: "--accent-label", background: "--window", required: 4.5 },
  { name: "accent text on a pressed toolbar button", foreground: "--accent-label", background: "--accent-soft", under: "--window", required: 4.5 },
  { name: "text on the prominent button", foreground: "--on-accent", background: "--accent", required: 4.5 },
  { name: "focus ring on the window", foreground: "--accent", background: "--window", required: 3 },
  { name: "danger text on the window", foreground: "--danger", background: "--window", required: 4.5 },
  { name: "a text field's fill against the window", foreground: "--control", background: "--window", required: 1 },
  // the unchosen part of a segmented control (sidebar, inspector tabs) on the glass panel
  { name: "secondary text on a control on glass", foreground: "--label-secondary", background: "--control", under: ["--glass", "--desk"], required: 4.5 },
];

export interface PairResult {
  name: string;
  mode: Mode;
  ratio: number;
  required: number;
  passes: boolean;
}

function solid(value: string | undefined): Colour | null {
  return value ? parseColour(value) : null;
}

/** the measured ratio of every pair in every mode. */
export function auditTokens(css: string, pairs: ContrastPair[] = PAIRS): PairResult[] {
  const results: PairResult[] = [];
  for (const mode of MODES) {
    const tokens = tokensFor(css, mode);
    for (const pair of pairs) {
      const front = solid(tokens[pair.foreground]);
      let back = solid(tokens[pair.background]);
      if (!front || !back) continue;
      // lay the layers down from the bottom up: the solid one first
      const layers = [pair.under ?? []].flat().map((name) => solid(tokens[name]));
      if (layers.some((layer) => !layer)) continue;
      const base = layers.reduceRight<Colour | null>((below, layer) => (below ? over(layer!, below) : layer), null);
      if (base && back.a < 1) back = over(back, base);
      const ratio = contrastRatio(front, back);
      results.push({ name: pair.name, mode, ratio, required: pair.required, passes: ratio >= pair.required });
    }
  }
  return results;
}

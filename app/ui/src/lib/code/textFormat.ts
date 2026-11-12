// what the status bar says about a file's text (encoding, line endings,
// indentation) and the size limits, in one place. the limits match the
// shell's (app/shell-core/src/code/text_file.rs).

export type LineEnding = "lf" | "crLf" | "cr";

export interface TextFormat {
  encoding: "utf8" | "utf16Le" | "utf16Be";
  byteOrderMark: boolean;
  lineEnding: LineEnding;
  mixedLineEndings: boolean;
  /** false when some bytes weren't valid text and show as "�" */
  valid: boolean;
}

export const FULL_FEATURES_LIMIT_BYTES = 2 * 1024 * 1024;

export function lineEndingLabel(ending: LineEnding): string {
  if (ending === "crLf") return "CRLF";
  if (ending === "cr") return "CR";
  return "LF";
}

export function encodingLabel(format: TextFormat): string {
  if (format.encoding === "utf16Le") return "UTF-16 LE";
  if (format.encoding === "utf16Be") return "UTF-16 BE";
  return format.byteOrderMark ? "UTF-8 with BOM" : "UTF-8";
}

/** the sentence under the encoding in the status bar's tooltip. */
export function formatSentence(format: TextFormat): string {
  const endings =
    format.lineEnding === "crLf" ? "Windows line endings (CRLF)" : format.lineEnding === "cr" ? "old Mac line endings (CR)" : "Mac and Linux line endings (LF)";
  const mixed = format.mixedLineEndings ? " The file mixes line endings; saving uses this one for every line." : "";
  return `Saved as ${encodingLabel(format)} with ${endings}, as the file was.${mixed}`;
}

export interface Indentation {
  /** true for tabs, false for spaces */
  tabs: boolean;
  /** spaces per level (also how wide a tab looks) */
  width: number;
}

const DEFAULT_INDENTATION: Indentation = { tabs: false, width: 4 };

/**
 * the indentation a file already uses: tabs when more indented lines start
 * with a tab, otherwise the most common step between one line's indent and
 * the next (2, 4 or 8 spaces).
 */
export function detectIndentation(text: string, fallback: Indentation = DEFAULT_INDENTATION): Indentation {
  const lines = text.slice(0, 200_000).split("\n");
  let tabLines = 0;
  let spaceLines = 0;
  const steps = new Map<number, number>();
  let previous = 0;
  for (const line of lines) {
    if (line.trim() === "") continue;
    if (line.startsWith("\t")) tabLines++;
    const spaces = /^ */.exec(line)?.[0].length ?? 0;
    if (spaces > 0) spaceLines++;
    const step = Math.abs(spaces - previous);
    if (step >= 2 && step <= 8) steps.set(step, (steps.get(step) ?? 0) + 1);
    previous = spaces;
  }
  if (tabLines === 0 && spaceLines === 0) return fallback;
  if (tabLines > spaceLines) return { tabs: true, width: fallback.width };
  let width = fallback.width;
  let best = 0;
  for (const [step, count] of steps) {
    if (count > best) {
      width = step;
      best = count;
    }
  }
  return { tabs: false, width };
}

export function indentationLabel(indentation: Indentation): string {
  return indentation.tabs ? `Tabs (${indentation.width} wide)` : `Spaces: ${indentation.width}`;
}

/** "1.2 MB", "340 KB", for the large file notice. */
export function sizeLabel(bytes: number): string {
  if (bytes >= 1024 * 1024) return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
  return `${Math.max(1, Math.round(bytes / 1024))} KB`;
}

/** the notice for a file too big for every feature, or null. */
export function largeFileNotice(bytes: number, readOnly: boolean): string | null {
  if (readOnly && bytes > FULL_FEATURES_LIMIT_BYTES) {
    return `This file is ${sizeLabel(bytes)}, so it opens read-only, without colours or checks. Letters edits files up to 32 MB.`;
  }
  if (bytes > FULL_FEATURES_LIMIT_BYTES) {
    return `This file is ${sizeLabel(bytes)}. To stay quick, colours and checks are off for files over 2 MB.`;
  }
  return null;
}

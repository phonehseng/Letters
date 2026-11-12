// choosing font files to add: the open panel, and telling font files
// apart from other files dropped on the window.

import { open as showOpenDialog } from "@tauri-apps/plugin-dialog";

export const FONT_EXTENSIONS = ["ttf", "otf", "ttc", "otc", "woff", "woff2"];

/** the dropped paths that look like font files (the engine checks them properly). */
export function fontFilesAmong(paths: string[]): string[] {
  const pattern = new RegExp(`\\.(${FONT_EXTENSIONS.join("|")})$`, "i");
  return paths.filter((path) => pattern.test(path));
}

/** the system's open panel for font files; several may be chosen. */
export async function chooseFontFiles(): Promise<string[]> {
  const chosen = await showOpenDialog({
    multiple: true,
    filters: [{ name: "Fonts", extensions: FONT_EXTENSIONS }],
  });
  if (!chosen) return [];
  return Array.isArray(chosen) ? chosen : [chosen];
}

/** the system's folder panel, for adding every font in a folder. */
export async function chooseFontFolder(): Promise<string | null> {
  const chosen = await showOpenDialog({ directory: true, multiple: false });
  return typeof chosen === "string" ? chosen : null;
}

/** a sentence about what adding fonts did, for the live region. */
export function addedSentence(report: { added: unknown[]; rejected: { file: string; reason: string }[]; alreadyAdded: string[] }): string {
  const parts: string[] = [];
  const faces = report.added.length;
  if (faces > 0) parts.push(faces === 1 ? "Added 1 font." : `Added ${faces} fonts.`);
  if (report.alreadyAdded.length > 0) {
    parts.push(report.alreadyAdded.length === 1 ? "1 font was already added." : `${report.alreadyAdded.length} fonts were already added.`);
  }
  if (report.rejected.length === 1) parts.push(report.rejected[0].reason);
  else if (report.rejected.length > 1) parts.push(`${report.rejected.length} files couldn’t be added. ${report.rejected[0].reason}`);
  return parts.join(" ") || "No fonts were found.";
}

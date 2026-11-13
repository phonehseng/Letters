// the commands of conversion and export, the AI Tools and Settings, by
// name, with their keys. they join the app's one list in ../commands.ts,
// so the menu bar and the keys reach them through the same dispatcher.

export const CONVERSION_COMMANDS = [
  // Convert File…: any file saved as another type (export/ConvertSheet.svelte)
  "convert-file",
  // Export To… for pdfs (ctrl or command, shift, e)
  "export-to",
  // "Open PDF as Document…" from the writer and the viewer
  "open-pdf-as-document",
  // Settings… (ctrl or command, comma, as on a mac)
  "open-settings",
  // the AI Tools panel, only while the tools are on in Settings
  "toggle-ai-tools",
  // Find a Topic…, which works on this computer without ai
  "find-topic",
] as const;

export type ConversionCommand = (typeof CONVERSION_COMMANDS)[number];

/** the keys, given the lower-case key and whether shift is down (ctrl or command is). */
export function conversionCommandForKey(key: string, shift: boolean): ConversionCommand | null {
  if (key === "," && !shift) return "open-settings";
  if (key === "e" && shift) return "export-to";
  if (key === "f" && shift) return "find-topic";
  return null;
}

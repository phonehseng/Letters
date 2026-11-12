// what the document menu (the pull-down on the document's name) holds,
// and which of its commands can be used right now.
//
// the same rules decide whether a keyboard shortcut does anything, so a
// command that looks disabled in the menu never runs from the keyboard.

import type { DocumentState } from "./api";
import type { Command } from "./commands";
import { tools, tooltipFor, type ToolHelp, type ToolName } from "./tools";

export interface DocumentMenuItem {
  command: Command;
  tool: ToolName;
  label: string;
  shortcut?: string;
  enabled: boolean;
  /** the usual tooltip, or why the command can't be used now */
  tooltip: string;
}

/** a line between groups of items */
export const SEPARATOR = "separator";

export type DocumentMenuEntry = DocumentMenuItem | typeof SEPARATOR;

const GROUPS: [Command, ToolName][][] = [
  [
    ["save", "save"],
    ["save-as", "saveAs"],
    // export: a copy in another format, or the text as a document (export/ExportSheet.svelte)
    ["export-to", "exportTo"],
    ["open-pdf-as-document", "openPdfAsDocument"],
  ],
  [
    ["undo", "undo"],
    ["redo", "redo"],
  ],
  [
    ["rotate-left", "rotateLeft"],
    ["rotate-right", "rotateRight"],
  ],
  // compare: this document as the newer version (compare/CompareScreen.svelte)
  [["compare-with-version", "compareWithVersion"]],
  // forms round two: pdf/a, preflight, smaller copies, print checks (standards/StandardsSheet.svelte)
  [["prepare-and-check", "prepareAndCheck"]],
  // code: the document as markdown, latex..., and the code inside it (code/DocumentCodeTools.svelte)
  [
    ["export-code-file", "exportCodeFile"],
    ["check-document-code", "checkDocumentCode"],
  ],
];

/** whether a document command can run on this document now. */
export function isAvailable(command: Command, document: Pick<DocumentState, "dirty" | "canUndo" | "canRedo" | "pages">): boolean {
  switch (command) {
    case "save":
      return document.dirty;
    case "undo":
      return document.canUndo;
    case "redo":
      return document.canRedo;
    case "rotate-left":
    case "rotate-right":
      return document.pages.length > 0;
    default:
      return true;
  }
}

/** the menu's entries for a document, in order, with separators between groups. */
export function documentMenuEntries(
  document: Pick<DocumentState, "dirty" | "canUndo" | "canRedo" | "pages">,
): DocumentMenuEntry[] {
  return GROUPS.flatMap((group, index) => {
    const items = group.map(([command, tool]): DocumentMenuItem => {
      const enabled = isAvailable(command, document);
      const help: ToolHelp = tools[tool];
      return {
        command,
        tool,
        label: help.label,
        shortcut: help.shortcut,
        enabled,
        tooltip: enabled ? tooltipFor(tool) : unavailableTooltip(tool),
      };
    });
    return index === 0 ? items : [SEPARATOR, ...items];
  });
}

function unavailableTooltip(tool: ToolName): string {
  const help: ToolHelp = tools[tool];
  return help.unavailable ?? tooltipFor(tool);
}

// what the writer's Explain panel lists: the open inspector tab's tools
// first, then everything else on screen.

import type { ToolName } from "../tools";

/** the writer inspector's tabs. */
export type WriterTab = "format" | "document" | "review";

export const TAB_TOOLS: Record<WriterTab, ToolName[]> = {
  format: [
    "wStyle", "wFont", "wFontSize", "wBold", "wItalic", "wUnderline", "wStrike", "wSuperscript", "wSubscript", "wTextColour", "wHighlight",
    "wAlignment", "wLineSpacing", "wSpacing", "wIndents", "wLists", "wTableRows", "wTableColumns", "wTableMerge", "wTableBorders",
    "wTableShading", "wTableHeaderRow", "wImageSize", "wImageAltText", "wImageWrap",
    "wImageCrop", "wImageRotate", "wImageLock", "wImageOriginalSize", "wImageFitWidth", "wBringToFront", "wSendToBack", "wAlignObjects",
  ],
  document: ["wPageSize", "wOrientation", "wMargins", "wColumns", "wHeaderFooter", "wPresetMla", "wPresetApa", "wAutocorrect", "wCapitalize", "wSmartQuotes", "wSpellcheck"],
  review: ["wTrackChanges", "wAcceptChange", "wRejectChange", "wComments", "wInsertComment"],
};

export const SCREEN_TOOLS: ToolName[] = [
  "wOutline", "wPromote", "wDemote", "recents", "documentMenu", "wSave", "wSaveAs", "wExportPdf", "wOpenAsPdf", "wOpenPdfAsDocument", "undo", "redo",
  "wStyle", "zoomOut", "zoomIn", "fitWidth", "wInsert", "wInsertTable", "wInsertPicture", "wInsertPageBreak", "wInsertFootnote", "wInsertLink",
  "wInsertPageNumber", "wInsertComment", "find", "wFindReplace", "wReplace", "wReplaceAll", "view", "wRuler", "wRulerFirstLine", "wRulerLeft",
  "wRulerRight", "wTabStops", "nightMode", "newWindow", "wFormat", "explain", "wStatusWords", "wStatusPages", "wStatusLanguage", "wAutocorrect",
  "wCapitalize", "wSmartQuotes", "wSpellcheck", "wTrackChanges", "wPreserved", "wStrictMode", "wLayoutSource", "wPrint", "wSaveToDrive", "wImageInline", "wImageFree", "wSaveFormats",
  "wTextBlockSplit", "wTextBlockMake", "wTextBlockDissolve", "wImageCrop", "wImageRotate", "wBringToFront", "wSendToBack", "wAlignObjects", "wCompatibility", "wCheckLayout",
];

export function writerExplainGroups(openTab: WriterTab | null): { title: string; names: ToolName[] }[] {
  if (!openTab) return [{ title: "On this screen", names: SCREEN_TOOLS }];
  const inTab = new Set(TAB_TOOLS[openTab]);
  const label = openTab === "format" ? "Format" : openTab === "document" ? "Document" : "Review";
  return [
    { title: `${label} tools`, names: TAB_TOOLS[openTab] },
    { title: "On this screen", names: SCREEN_TOOLS.filter((name) => !inTab.has(name)) },
  ];
}

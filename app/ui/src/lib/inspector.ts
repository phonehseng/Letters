// the Tools inspector: the panel on the trailing side whose tabs hold the
// editing tools, as the inspector in Pages does instead of a ribbon.
//
// adding a tab is one entry in INSPECTOR_TABS plus its component:
//   1. write lib/inspector/<Name>Tab.svelte. it gets one prop, `context`
//      (a ToolContext below), and is built from ToolSection components.
//   2. add an entry here: id, label, tooltip, the tools it holds (each
//      one needs an entry in tools.ts, which Explain shows), and `load`,
//      which imports the component only when the tab is first opened.
// the segmented control, Explain and the menu commands pick it up from here.

import type { Component } from "svelte";
import type { AnnotateSession } from "./annotate/annotateSession.svelte";
import type { DocumentState } from "./api";
import type { CropBox } from "./cropRect";
import type { FillSignSession } from "./forms/fillSignSession.svelte";
import type { ProtectSession } from "./protect/protectSession.svelte";
import type { A11ySession } from "./a11y/a11ySession.svelte";
import type { EditSession } from "./edit/editSession.svelte";
import type { ScanSession } from "./ocr/scanSession.svelte";
import { tools, type ToolName } from "./tools";

/** what every tool in the inspector can see and do. */
export interface ToolContext {
  document: DocumentState;
  /** the page being read, counted from 0 */
  currentPage: number;
  /** the pages a tool acts on: the selected ones, or else the page being read */
  target: { pages: number[]; fromSelection: boolean };
  /**
   * runs a change to this document through the window's queue and shows
   * the result. resolves to true when it worked; a failure has already
   * been shown to the person.
   */
  change: (action: () => Promise<DocumentState>) => Promise<boolean>;
  /** a short message under the toolbar, with an optional button such as "Open" */
  notify: (message: string, action?: { label: string; run: () => void }) => void;
  showError: (message: string) => void;
  /** a section a menu command asked to open; `at` makes each request new */
  requested: { tool: ToolName; at: number } | null;
  /** the rectangle drawn on the page while the Crop tool is open */
  crop: {
    box: CropBox | null;
    setBox: (box: CropBox) => void;
    /** the crop tool shows the rectangle on the page while it is open */
    setActive: (active: boolean) => void;
  };
  /** the Annotate tools' shared state: the page tool, selections, styles (see annotateSession.svelte.ts) */
  annotate: AnnotateSession;
  /** the Fill & Sign tools' shared state: the form, the field being filled, signatures (see forms/fillSignSession.svelte.ts) */
  fillSign?: FillSignSession;
  /** the Protect tools' shared state: marks for redaction, the review, passwords (see protect/protectSession.svelte.ts) */
  protect?: ProtectSession;
  /** the Accessibility tab's shared state: the report, the fix wizard, what is outlined (see a11y/a11ySession.svelte.ts) */
  a11y?: A11ySession;
  /** the Edit tab's shared state: text edited in place, pictures (see edit/editSession.svelte.ts) */
  edit?: EditSession;
  /** the Scan tab's shared state: recognition runs and recognized words (see ocr/scanSession.svelte.ts) */
  scan?: ScanSession;
}

export interface InspectorTabProps {
  context: ToolContext;
}

export interface InspectorTab {
  id: string;
  label: string;
  tooltip: string;
  /** the tools in this tab, in the order they are shown; Explain lists them */
  tools: ToolName[];
  load: () => Promise<{ default: Component<InspectorTabProps> }>;
}

export const INSPECTOR_TABS: InspectorTab[] = [
  {
    id: "pages",
    label: "Pages",
    tooltip: "Rotate, delete, add, crop, number and watermark pages; split and merge PDFs",
    tools: [
      "rotatePages",
      "deletePages",
      "insertPages",
      "extractPages",
      "splitDocument",
      "mergeDocuments",
      "cropPages",
      "pageNumbers",
      "headerFooter",
      "watermark",
      "removeStamps",
    ],
    load: () => import("./inspector/PagesTab.svelte"),
  },
  {
    id: "annotate",
    label: "Annotate",
    tooltip: "Highlight text, add notes, text boxes, shapes, drawings and pictures",
    tools: [
      "editAnnotation",
      "highlightText",
      "underlineText",
      "strikeText",
      "addNote",
      "addTextBox",
      "drawShapes",
      "drawInk",
      "addPicture",
      "comments",
      "flattenComments",
      "authorName",
    ],
    load: () => import("./inspector/AnnotateTab.svelte"),
  },
  // forms and signatures (see forms/fillSignSession.svelte.ts)
  {
    id: "fillSign",
    label: "Fill & Sign",
    tooltip: "Fill in forms, find and add fields, and sign with a picture or a certificate",
    tools: [
      "fillForm",
      "showFields",
      "nextEmptyField",
      "findFields",
      "addField",
      "editFields",
      "flattenForm",
      "signWithPicture",
      "signWithCertificate",
      "selfSignedCertificate",
      "signatures",
    ],
    load: () => import("./inspector/FillSignTab.svelte"),
  },
  // redaction, hidden information and passwords (see protect/protectSession.svelte.ts)
  {
    id: "protect",
    label: "Protect",
    tooltip: "Redact text and pictures, remove hidden information, and add a password",
    tools: [
      "redactMarkText",
      "redactMarkArea",
      "redactFind",
      "redactReview",
      "redactApply",
      "removeHiddenInfo",
      "unlockForEditing",
      "openPassword",
      "restrictPermissions",
      "removePassword",
    ],
    load: () => import("./inspector/ProtectTab.svelte"),
  },
  // the accessibility checker and fix wizard (see a11y/a11ySession.svelte.ts)
  {
    id: "accessibility",
    label: "Accessibility",
    tooltip: "Check how well this PDF works with screen readers, and fix what Letters can",
    tools: ["checkAccessibility", "fixAccessibility", "addTagsAutomatically", "markPdfUa", "saveAccessibilityReport", "showOnPage"],
    load: () => import("./inspector/AccessibilityTab.svelte"),
  },
  // editing the page's own text and pictures (see edit/editSession.svelte.ts)
  {
    id: "edit",
    label: "Edit",
    tooltip: "Change the words on the page in place, add text, and move, resize or replace pictures",
    // edit round two adds pageShapes and pageSetup at the end
    tools: ["editText", "editTextStyle", "deletePageText", "addPageText", "pageImages", "addPageImage", "pageShapes", "pageSetup"],
    load: () => import("./inspector/EditTab.svelte"),
  },
  // text recognition for scans and photos (see ocr/scanSession.svelte.ts)
  {
    id: "scan",
    label: "Scan",
    tooltip: "Make scanned pages searchable, check what was read, and make a PDF from pictures",
    tools: ["recognizeText", "showRecognizedText", "removeRecognizedText", "newFromImages"],
    load: () => import("./inspector/ScanTab.svelte"),
  },
];

export function tabById(id: string): InspectorTab {
  return INSPECTOR_TABS.find((tab) => tab.id === id) ?? INSPECTOR_TABS[0];
}

/** the tab that holds a tool, so a menu command can open it there. */
export function tabWithTool(tool: ToolName): InspectorTab | null {
  return INSPECTOR_TABS.find((tab) => tab.tools.includes(tool)) ?? null;
}

/** a group of tools in the Explain panel, under a caption. */
export interface ExplainGroup {
  title: string;
  names: ToolName[];
}

/**
 * what Explain shows: the tools of the open inspector tab first (that is
 * what the person is working with), then everything else on screen.
 */
export function explainGroups(onScreen: ToolName[], openTab: InspectorTab | null): ExplainGroup[] {
  const screen: ExplainGroup = { title: "On this screen", names: onScreen.filter((name) => name in tools) };
  if (!openTab) return [screen];
  const inTab = new Set(openTab.tools);
  return [
    { title: `${openTab.label} tools`, names: openTab.tools },
    { ...screen, names: screen.names.filter((name) => !inTab.has(name)) },
  ];
}

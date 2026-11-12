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
import type { DocumentState } from "./api";
import type { CropBox } from "./cropRect";
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

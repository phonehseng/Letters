import { describe, expect, it } from "vitest";
import { explainGroups, INSPECTOR_TABS, tabById, tabWithTool } from "./inspector";
import { tools } from "./tools";

describe("the inspector registry", () => {
  it("gives every tab a unique id and only tools that have descriptions", () => {
    const ids = INSPECTOR_TABS.map((tab) => tab.id);
    expect(new Set(ids).size).toBe(ids.length);
    for (const tab of INSPECTOR_TABS) {
      expect(tab.tools.length).toBeGreaterThan(0);
      for (const tool of tab.tools) expect(tools[tool], tool).toBeDefined();
      expect(typeof tab.load).toBe("function");
    }
  });

  it("starts with the Pages tab", () => {
    expect(INSPECTOR_TABS[0].label).toBe("Pages");
    expect(tabById("unknown")).toBe(INSPECTOR_TABS[0]);
  });

  it("finds the tab that holds a tool", () => {
    expect(tabWithTool("cropPages")?.id).toBe("pages");
    expect(tabWithTool("zoomIn")).toBeNull();
  });

  it("has an Annotate tab after Pages with every annotation tool", () => {
    expect(INSPECTOR_TABS.map((tab) => tab.label).slice(0, 2)).toEqual(["Pages", "Annotate"]);
    for (const tool of ["highlightText", "addNote", "addTextBox", "drawInk", "flattenComments"] as const) {
      expect(tabWithTool(tool)?.id).toBe("annotate");
    }
  });

  // forms and signatures
  it("has a Fill & Sign tab after Annotate with every form and signing tool", () => {
    expect(INSPECTOR_TABS[2].label).toBe("Fill & Sign");
    for (const tool of ["fillForm", "findFields", "addField", "editFields", "flattenForm", "signWithPicture", "signWithCertificate", "signatures"] as const) {
      expect(tabWithTool(tool)?.id).toBe("fillSign");
    }
  });
});

describe("explainGroups", () => {
  it("lists the open tab's tools first, then the rest of the screen without repeats", () => {
    const pages = tabById("pages");
    const groups = explainGroups(["sidebar", "deletePages", "explain"], pages);
    expect(groups[0]).toEqual({ title: "Pages tools", names: pages.tools });
    expect(groups[1]).toEqual({ title: "On this screen", names: ["sidebar", "explain"] });
  });

  it("explains only the screen when no tab is open", () => {
    expect(explainGroups(["sidebar"], null)).toEqual([{ title: "On this screen", names: ["sidebar"] }]);
  });
});

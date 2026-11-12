import { describe, expect, it } from "vitest";
import { documentMenuEntries, isAvailable, SEPARATOR, type DocumentMenuItem } from "./documentMenu";
import { tools } from "./tools";

const page = { width: 612, height: 792 };
const fresh = { dirty: false, canUndo: false, canRedo: false, pages: [page] };
const edited = { dirty: true, canUndo: true, canRedo: false, pages: [page] };

const items = (entries: ReturnType<typeof documentMenuEntries>) =>
  entries.filter((entry): entry is DocumentMenuItem => entry !== SEPARATOR);

describe("documentMenuEntries", () => {
  it("lists save, history, rotation and compare in four groups", () => {
    const entries = documentMenuEntries(fresh);
    expect(entries.map((entry) => (entry === SEPARATOR ? "—" : entry.command))).toEqual([
      "save",
      "save-as",
      // export (export/ExportSheet.svelte)
      "export-to",
      "open-pdf-as-document",
      "—",
      "undo",
      "redo",
      "—",
      "rotate-left",
      "rotate-right",
      "—",
      "compare-with-version",
      // code (code/DocumentCodeTools.svelte)
      "—",
      "export-code-file",
      "check-document-code",
    ]);
  });

  it("disables what can't be used and says why", () => {
    const byCommand = new Map(items(documentMenuEntries(fresh)).map((item) => [item.command, item]));
    expect(byCommand.get("undo")).toMatchObject({ enabled: false, tooltip: "Nothing to undo" });
    expect(byCommand.get("redo")).toMatchObject({ enabled: false, tooltip: "Nothing to redo" });
    expect(byCommand.get("save")).toMatchObject({ enabled: false, tooltip: tools.save.unavailable });
    // save as always works: it is also the way out when the file is locked
    expect(byCommand.get("save-as")?.enabled).toBe(true);
    expect(byCommand.get("rotate-right")?.enabled).toBe(true);
  });

  it("enables save and undo once there is a change, with the usual tooltips", () => {
    const byCommand = new Map(items(documentMenuEntries(edited)).map((item) => [item.command, item]));
    expect(byCommand.get("save")).toMatchObject({ enabled: true, tooltip: "Save the changes to this document (Ctrl S)" });
    expect(byCommand.get("undo")).toMatchObject({ enabled: true, tooltip: "Take back the last change (Ctrl Z)" });
  });
});

describe("isAvailable", () => {
  it("matches the menu, so a shortcut never runs a disabled command", () => {
    expect(isAvailable("undo", fresh)).toBe(false);
    expect(isAvailable("undo", edited)).toBe(true);
    expect(isAvailable("rotate-left", { ...fresh, pages: [] })).toBe(false);
    expect(isAvailable("zoom-in", fresh)).toBe(true);
  });
});

describe("the words for every document tool", () => {
  it("say exactly when the file on disk changes", () => {
    for (const name of ["undo", "redo", "rotateLeft", "rotateRight"] as const) {
      expect(tools[name].fileEffect).toContain("The file on disk changes only when you choose Save.");
    }
    expect(tools.save.fileEffect).toContain("Replaces the file on disk");
    expect(tools.saveAs.fileEffect).toContain("The file you opened stays as it is on disk");
  });
});

import { describe, expect, it } from "vitest";
import type { PinnedFolder } from "./api";
import { filterFolders } from "./folders";

const folder = (name: string, files: string[]): PinnedFolder => ({
  path: `C:\\Courses\\${name}`,
  name,
  files: files.map((file) => ({ path: `C:\\Courses\\${name}\\${file}`, name: file, modifiedAt: 0 })),
  total: files.length,
});

const folders = [
  folder("Biology", ["Lab Report 3.pdf", "Cells.pdf"]),
  folder("History", ["Essay draft.pdf"]),
  folder("Empty", []),
];

describe("filterFolders", () => {
  it("shows every folder and file when not searching, empty ones too", () => {
    const shown = filterFolders(folders, "  ");
    expect(shown.map((entry) => entry.folder.name)).toEqual(["Biology", "History", "Empty"]);
    expect(shown[0].files).toHaveLength(2);
  });

  it("keeps only matching files and hides folders without any", () => {
    const shown = filterFolders(folders, "report");
    expect(shown).toHaveLength(1);
    expect(shown[0].folder.name).toBe("Biology");
    expect(shown[0].files.map((file) => file.name)).toEqual(["Lab Report 3.pdf"]);
  });

  it("shows nothing when no file matches", () => {
    expect(filterFolders(folders, "chemistry")).toEqual([]);
  });
});

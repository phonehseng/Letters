import { describe, expect, it } from "vitest";
import type { DocumentState } from "./api";
import {
  choiceForKey,
  createSerialQueue,
  mayLeave,
  placeAfterEdit,
  rotation,
  unsavedChangesQuestion,
  windowTitle,
  type SaveChoice,
} from "./documentState";

const documentState = (extra: Partial<DocumentState> = {}): DocumentState => ({
  id: 1,
  name: "essay.pdf",
  path: "C:\\notes\\essay.pdf",
  pages: [{ width: 612, height: 792 }],
  revision: 0,
  dirty: false,
  canUndo: false,
  canRedo: false,
  ...extra,
});

describe("windowTitle", () => {
  it("names the document, and says when it has unsaved changes", () => {
    expect(windowTitle(documentState())).toBe("essay.pdf");
    expect(windowTitle(documentState({ dirty: true }))).toBe("essay.pdf — Edited");
    expect(windowTitle(null)).toBe("Recents");
  });
});

describe("placeAfterEdit", () => {
  it("keeps the reader's spot when the page still exists", () => {
    expect(placeAfterEdit({ page: 3, fraction: 0.4 }, 10)).toEqual({ page: 3, fraction: 0.4 });
  });

  it("goes to the top of the last page when the page is gone", () => {
    expect(placeAfterEdit({ page: 9, fraction: 0.4 }, 5)).toEqual({ page: 4, fraction: 0 });
    expect(placeAfterEdit({ page: 2, fraction: 0.5 }, 0)).toEqual({ page: 0, fraction: 0 });
  });
});

describe("rotation", () => {
  it("turns the current page a quarter turn either way", () => {
    expect(rotation("left", 4)).toEqual({ pages: [4], degrees: -90 });
    expect(rotation("right", 0)).toEqual({ pages: [0], degrees: 90 });
  });
});

describe("createSerialQueue", () => {
  it("runs tasks one at a time, in the order they were given", async () => {
    const run = createSerialQueue();
    const log: string[] = [];
    let finishFirst = () => {};
    const first = run(
      () =>
        new Promise<void>((resolve) => {
          log.push("first starts");
          finishFirst = () => {
            log.push("first ends");
            resolve();
          };
        }),
    );
    const second = run(async () => {
      log.push("second starts");
    });
    await Promise.resolve();
    expect(log).toEqual(["first starts"]);
    finishFirst();
    await Promise.all([first, second]);
    expect(log).toEqual(["first starts", "first ends", "second starts"]);
  });

  it("keeps going after a task fails, and passes the failure to its caller", async () => {
    const run = createSerialQueue();
    const failed = run(async () => {
      throw new Error("no");
    });
    await expect(failed).rejects.toThrow("no");
    await expect(run(async () => 42)).resolves.toBe(42);
  });
});

describe("the unsaved changes sheet", () => {
  it("asks in the mac's words", () => {
    expect(unsavedChangesQuestion("essay.pdf")).toEqual({
      title: "Do you want to save the changes made to “essay.pdf”?",
      message: "Your changes will be lost if you don’t save them.",
    });
  });

  it("saves on Enter and cancels on Escape", () => {
    expect(choiceForKey("Enter")).toBe("save");
    expect(choiceForKey("Escape")).toBe("cancel");
    expect(choiceForKey("a")).toBeNull();
  });
});

describe("mayLeave", () => {
  const answer = (choice: SaveChoice) => async () => choice;
  const neverAsked = async (): Promise<SaveChoice> => {
    throw new Error("should not ask");
  };

  it("leaves straight away when nothing is unsaved", async () => {
    expect(await mayLeave(documentState(), neverAsked, async () => false)).toBe(true);
    expect(await mayLeave(null, neverAsked, async () => false)).toBe(true);
  });

  it("follows the answer when there are unsaved changes", async () => {
    const edited = documentState({ dirty: true });
    let saves = 0;
    const save = async () => {
      saves += 1;
      return true;
    };
    expect(await mayLeave(edited, answer("cancel"), save)).toBe(false);
    expect(await mayLeave(edited, answer("dont-save"), save)).toBe(true);
    expect(saves).toBe(0);
    expect(await mayLeave(edited, answer("save"), save)).toBe(true);
    expect(saves).toBe(1);
  });

  it("stays when saving fails, so nothing is lost", async () => {
    const edited = documentState({ dirty: true });
    expect(await mayLeave(edited, answer("save"), async () => false)).toBe(false);
  });
});

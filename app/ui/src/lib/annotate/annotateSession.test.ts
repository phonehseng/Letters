import { beforeEach, describe, expect, it, vi } from "vitest";
import type { DocumentState } from "../api";
import type { PageText } from "../pageTools/textSelection";
import type { AnnotationEdit } from "./annotateApi";

// the session talks to the shell through annotateApi; here every call is
// recorded and answered with a made-up state instead
const calls: { command: string; args: unknown[] }[] = [];
const state: DocumentState = { id: 1, name: "a.pdf", path: "a.pdf", pages: [], revision: 2, dirty: true, canUndo: true, canRedo: false };

vi.mock("./annotateApi", () => {
  const made = (kind: string, page: number): AnnotationEdit => ({
    document: state,
    annotation: {
      id: `made-${calls.length}`,
      page,
      kind: kind as never,
      subtype: kind,
      rect: { x: 0, y: 0, width: 10, height: 10 },
      color: null,
      opacity: 1,
      contents: null,
      author: null,
      modified: null,
      created: null,
      replyTo: null,
      hidden: false,
      editable: true,
      details: { type: "none" },
    },
  });
  const record = (command: string) => (...args: unknown[]) => {
    calls.push({ command, args });
    return Promise.resolve(made(command === "addShape" ? "square" : "note", args[1] as number));
  };
  return {
    addTextMarkups: (...args: unknown[]) => {
      calls.push({ command: "addTextMarkups", args });
      return Promise.resolve({ document: state, annotations: [] });
    },
    addShape: record("addShape"),
    addNote: record("addNote"),
    listAllAnnotations: () => Promise.resolve([]),
    pageText: () => Promise.resolve({ chars: [], lines: [] }),
  };
});

const { AnnotateSession } = await import("./annotateSession.svelte");

function lineOf(text: string, y: number): PageText {
  return {
    chars: [...text].map((ch, index) => ({ ch, rect: { x: 72 + 10 * index, y, width: 10, height: 20 }, line: 0 })),
    lines: [{ x: 72, y, width: 10 * text.length, height: 20 }],
  };
}

function makeSession() {
  const announced: string[] = [];
  const session = new AnnotateSession({
    documentId: () => 1,
    pageSize: () => ({ width: 612, height: 792 }),
    pageCount: () => 2,
    currentPage: () => 0,
    viewCentre: () => ({ page: 0, x: 306, y: 396 }),
    change: async (action) => {
      await action();
      return true;
    },
    announce: (text) => announced.push(text),
    reveal: () => {},
  });
  return { session, announced };
}

beforeEach(() => {
  calls.length = 0;
});

describe("the annotate session", () => {
  it("marks a selection across two pages in one edit, so one undo takes it back", async () => {
    const { session } = makeSession();
    // the store has no text yet; the session reads it through its getter
    const texts = new Map([
      [0, lineOf("Cells need light", 100)],
      [1, lineOf("Second page", 100)],
    ]);
    session.texts.get = (page: number) => texts.get(page);
    session.selection = {
      anchor: { page: 0, offset: 6 },
      focus: { page: 1, offset: 6 },
      granularity: "character",
      origin: { start: { page: 0, offset: 6 }, end: { page: 0, offset: 6 } },
    };
    expect(await session.applyMark("highlight")).toBe(true);
    expect(calls).toHaveLength(1);
    const marks = calls[0].args[1] as { page: number; params: { rects: unknown[] } }[];
    expect(marks.map((mark) => mark.page)).toEqual([0, 1]);
    expect(session.selection).toBeNull();
  });

  it("lets go of one thing per Escape: a tool, then the selected object, then the text selection", () => {
    const { session } = makeSession();
    session.setTool("draw");
    session.selectedId = "x";
    session.selection = { anchor: { page: 0, offset: 0 }, focus: { page: 0, offset: 1 }, granularity: "character", origin: { start: { page: 0, offset: 0 }, end: { page: 0, offset: 0 } } };
    expect(session.escape()).toBe(true);
    expect(session.tool).toBe("select");
    session.selectedId = "x";
    expect(session.escape()).toBe(true);
    expect(session.selectedId).toBeNull();
    session.selection = { anchor: { page: 0, offset: 0 }, focus: { page: 0, offset: 1 }, granularity: "character", origin: { start: { page: 0, offset: 0 }, end: { page: 0, offset: 0 } } };
    expect(session.escape()).toBe(true);
    expect(session.selection).toBeNull();
    expect(session.escape()).toBe(false);
  });

  it("puts a note in the middle of the view from the keyboard, ready to type", async () => {
    const { session } = makeSession();
    await session.placeFromKeyboard("note");
    // the icon is centred on the middle of the view
    expect(session.note).toEqual({ page: 0, position: { x: 296, y: 386 }, id: null, text: "" });
    expect(await session.saveNote("Hello")).toBe(true);
    expect(calls[0].command).toBe("addNote");
  });

  it("keeps a note's typed words when a click elsewhere closes it", async () => {
    const { session } = makeSession();
    await session.placeFromKeyboard("note");
    session.noteTyped = "Ask about the control group";
    // what a click on empty page does: keep the words, then close the note
    session.keepTypedNote();
    session.note = null;
    await vi.waitFor(() => expect(calls).toHaveLength(1));
    expect(calls[0].command).toBe("addNote");
    expect((calls[0].args[2] as { contents: string }).contents).toBe("Ask about the control group");
    // the closed note doesn't open again once it is saved
    await Promise.resolve();
    expect(session.note).toBeNull();
  });

  it("keeps a note's typed words when another tool is chosen", async () => {
    const { session } = makeSession();
    await session.placeFromKeyboard("note");
    session.noteTyped = "Check the graph";
    session.setTool("highlight");
    await vi.waitFor(() => expect(calls).toHaveLength(1));
    expect((calls[0].args[2] as { contents: string }).contents).toBe("Check the graph");
    expect(session.note).toBeNull();
  });

  it("leaves no note behind when a new one is closed empty", async () => {
    const { session } = makeSession();
    await session.placeFromKeyboard("note");
    session.noteTyped = "   ";
    session.keepTypedNote();
    session.note = null;
    await Promise.resolve();
    expect(calls).toHaveLength(0);
  });

  it("adds a shape from the keyboard, selects it and says so", async () => {
    const { session, announced } = makeSession();
    await session.placeFromKeyboard("shape");
    expect(calls[0].command).toBe("addShape");
    const params = calls[0].args[2] as { rect: { x: number; width: number } };
    expect(params.rect.x + params.rect.width / 2).toBe(306);
    expect(session.selectedId).toMatch(/^made-/);
    expect(announced).toEqual(["Rectangle added to page 1."]);
    // a tool selecting what it made doesn't count as the person picking it
    expect(session.picks).toBe(0);
  });

  it("keeps a text selection for the mark tools, and drops it for the others", () => {
    const { session } = makeSession();
    const selection = { anchor: { page: 0, offset: 0 }, focus: { page: 0, offset: 1 }, granularity: "character" as const, origin: { start: { page: 0, offset: 0 }, end: { page: 0, offset: 0 } } };
    session.selection = selection;
    session.setTool("highlight");
    expect(session.selection).toEqual(selection);
    session.setTool("shape");
    expect(session.selection).toBeNull();
  });
});

import { describe, expect, it } from "vitest";
import { STROKE_GROUP_PAUSE_MS } from "./gestures";
import type { PagePointer } from "./pageTool";
import { inkTool, lineTool, pointTool, rectangleTool, textTool } from "./tools.svelte";
import { selectedText, type PageText, type TextSelection } from "./textSelection";
import type { Point, Rect } from "./pagePoints";

const letter = { width: 612, height: 792 };
const style = { stroke: "black", fill: null, width: 1, opacity: 1 };

/** the parts of a key press the tools read; tests run without a browser */
function key(press: Partial<KeyboardEvent>): KeyboardEvent {
  return { shiftKey: false, ctrlKey: false, altKey: false, metaKey: false, ...press } as KeyboardEvent;
}

function pointer(x: number, y: number, extra: Partial<PagePointer> = {}): PagePointer {
  return { page: 0, x, y, scale: 1, shiftKey: false, altKey: false, clicks: 1, time: 0, ...extra };
}

describe("the rectangle tool", () => {
  function make(clickSize: { width: number; height: number } | null = null) {
    const drawn: Rect[] = [];
    const tool = rectangleTool({
      id: "test-rect",
      style: () => style,
      pageSize: () => letter,
      squareWithShift: true,
      clickSize: () => clickSize,
      onDraw: (_page, rect) => drawn.push(rect),
    });
    return { tool, drawn };
  }

  it("previews while dragging and draws on release", () => {
    const { tool, drawn } = make();
    const gesture = tool.down(pointer(100, 100))!;
    gesture.move(pointer(150, 130));
    expect(tool.marks(0)).toEqual([{ kind: "rect", rect: { x: 100, y: 100, width: 50, height: 30 }, style }]);
    expect(tool.marks(1)).toEqual([]);
    gesture.up(pointer(150, 130));
    expect(drawn).toEqual([{ x: 100, y: 100, width: 50, height: 30 }]);
    expect(tool.marks(0)).toEqual([]);
  });

  it("draws a square with shift held", () => {
    const { tool, drawn } = make();
    tool.down(pointer(100, 100))!.up(pointer(150, 120, { shiftKey: true }));
    expect(drawn).toEqual([{ x: 100, y: 100, width: 50, height: 50 }]);
  });

  it("ignores a click, or places a default size when asked to", () => {
    const quiet = make();
    quiet.tool.down(pointer(100, 100))!.up(pointer(100, 100));
    expect(quiet.drawn).toEqual([]);
    const placing = make({ width: 100, height: 50 });
    placing.tool.down(pointer(100, 100))!.up(pointer(100, 100));
    expect(placing.drawn).toEqual([{ x: 50, y: 75, width: 100, height: 50 }]);
  });

  it("drops the drag on escape", () => {
    const { tool, drawn } = make();
    const gesture = tool.down(pointer(100, 100))!;
    gesture.move(pointer(200, 200));
    gesture.cancel();
    expect(tool.marks(0)).toEqual([]);
    expect(drawn).toEqual([]);
  });
});

describe("the line and point tools", () => {
  it("draws a line from a drag", () => {
    const lines: [Point, Point][] = [];
    const tool = lineTool({ id: "line", style: () => style, arrow: () => true, pageSize: () => letter, onDraw: (_p, a, b) => lines.push([a, b]) });
    const gesture = tool.down(pointer(10, 10))!;
    gesture.move(pointer(60, 10));
    expect(tool.marks(0)[0]).toMatchObject({ kind: "line", arrow: true });
    gesture.up(pointer(60, 10));
    expect(lines).toEqual([[{ x: 10, y: 10 }, { x: 60, y: 10 }]]);
  });

  it("places a point on a click", () => {
    const placed: Point[] = [];
    const tool = pointTool({ id: "note", onPlace: (_page, point) => placed.push(point) });
    tool.down(pointer(30, 40))!.up(pointer(30, 40));
    expect(placed).toEqual([{ x: 30, y: 40 }]);
  });
});

describe("the ink tool", () => {
  function make() {
    let clock = 0;
    const timers: (() => void)[] = [];
    const drawings: { page: number; strokes: Point[][] }[] = [];
    const tool = inkTool({
      id: "ink",
      style: () => ({ style, key: "black-2" }),
      onDrawing: (page, strokes) => drawings.push({ page, strokes }),
      now: () => clock,
      later: (run) => timers.push(run),
    });
    const stroke = (from: number, page = 0) => {
      const gesture = tool.down(pointer(from, from, { page }))!;
      gesture.move(pointer(from + 10, from, { page }));
      gesture.up(pointer(from + 20, from, { page }));
    };
    const wait = (ms: number) => {
      clock += ms;
      timers.splice(0).forEach((run) => run());
    };
    return { tool, drawings, stroke, wait };
  }

  it("saves strokes drawn without a pause as one drawing", () => {
    const { tool, drawings, stroke, wait } = make();
    stroke(10);
    wait(200);
    stroke(40);
    expect(drawings).toEqual([]);
    // both strokes stay on screen while the drawing waits
    expect((tool.marks(0)[0] as { strokes: Point[][] }).strokes).toHaveLength(2);
    wait(STROKE_GROUP_PAUSE_MS + 100);
    expect(drawings).toHaveLength(1);
    expect(drawings[0].strokes).toHaveLength(2);
    expect(tool.marks(0)).toEqual([]);
  });

  it("starts a new drawing on another page and saves the first", () => {
    const { drawings, stroke } = make();
    stroke(10, 0);
    stroke(10, 1);
    expect(drawings.map((drawing) => drawing.page)).toEqual([0]);
  });

  it("saves what is waiting when the tool is put away", () => {
    const { tool, drawings, stroke } = make();
    stroke(10);
    tool.finish?.();
    expect(drawings).toHaveLength(1);
  });
});

describe("the text tool", () => {
  const page: PageText = {
    chars: [..."Cells need light"].map((ch, index) => ({ ch, rect: { x: 72 + 10 * index, y: 100, width: 10, height: 20 }, line: 0 })),
    lines: [{ x: 72, y: 100, width: 160, height: 20 }],
  };

  function make() {
    let selection: TextSelection | null = null;
    const finished: string[] = [];
    const host = {
      text: () => page,
      selection: () => selection,
      setSelection: (next: TextSelection | null) => (selection = next),
      currentPage: () => 0,
      pageCount: () => 1,
    };
    const tool = textTool({ id: "select", host, onSelected: (done) => finished.push(selectedText(done, host.text)) });
    return { tool, finished, current: () => selection };
  }

  it("selects text by dragging and reports it when the drag ends", () => {
    const { tool, finished } = make();
    const gesture = tool.down(pointer(72, 110))!;
    gesture.move(pointer(121, 110));
    gesture.up(pointer(121, 110));
    expect(finished).toEqual(["Cells"]);
  });

  it("clears the selection on a plain click", () => {
    const { tool, current } = make();
    tool.down(pointer(72, 110))!.up(pointer(72, 110));
    expect(current()).toBeNull();
  });

  it("selects a word on a double click", () => {
    const { tool, finished } = make();
    tool.down(pointer(135, 110, { clicks: 2 }))!.up(pointer(135, 110, { clicks: 2 }));
    expect(finished).toEqual(["need"]);
  });

  it("extends with shift and the arrows", () => {
    const { tool, current } = make();
    const handled = tool.key?.(key({ key: "ArrowRight", shiftKey: true, ctrlKey: true }));
    expect(handled).toBe(true);
    expect(current()?.focus.offset).toBe(5);
    expect(tool.key?.(key({ key: "ArrowRight" }))).toBe(false);
  });

  it("lets an object under the pointer take the press first", () => {
    let selection: TextSelection | null = null;
    const host = { text: () => page, selection: () => selection, setSelection: (s: TextSelection | null) => (selection = s), currentPage: () => 0, pageCount: () => 1 };
    const objectGesture = { move() {}, up() {}, cancel() {} };
    const tool = textTool({ id: "select", host, pick: () => objectGesture });
    expect(tool.down(pointer(72, 110))).toBe(objectGesture);
    expect(selection).toBeNull();
  });
});

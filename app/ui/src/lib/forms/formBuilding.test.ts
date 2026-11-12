// Prepare Form's building panes: moving fields in the tab order list,
// which fields can be grouped, where buttons go, and the sentences.

import { describe, expect, it } from "vitest";
import { buttonPlace, BUTTON_SIZE, fieldsOnPage, groupable, groupEffect, groupNameProblem, moveItem } from "./formBuilding";
import type { FieldInfo } from "./formsApi";

function field(name: string, kind: FieldInfo["kind"] = "text"): FieldInfo {
  return { name, kind, tooltip: null } as FieldInfo;
}

describe("the tab order list", () => {
  it("moves one field and keeps the rest in order", () => {
    expect(moveItem(["a", "b", "c", "d"], 3, 1)).toEqual(["a", "d", "b", "c"]);
    expect(moveItem(["a", "b", "c"], 0, 2)).toEqual(["b", "c", "a"]);
  });

  it("changes nothing for a move past either end", () => {
    const list = ["a", "b"];
    expect(moveItem(list, 0, -1)).toBe(list);
    expect(moveItem(list, 1, 2)).toBe(list);
  });

  it("lists the page's fields in the engine's order, skipping names it doesn't know", () => {
    const fields = [field("a"), field("b"), field("c")];
    expect(fieldsOnPage(["c", "gone", "a"], fields).map((each) => each.name)).toEqual(["c", "a"]);
  });
});

describe("groups", () => {
  it("only top-level fields that aren't signatures can join a group", () => {
    const fields = [field("street"), field("address.city"), field("sign", "signature")];
    expect(groupable(fields).map((each) => each.name)).toEqual(["street"]);
  });

  it("a group name must be new and have no full stop", () => {
    const fields = [field("street")];
    expect(groupNameProblem(" ", fields)).toBe("Type a name for the group, such as address.");
    expect(groupNameProblem("a.b", fields)).toBe("A group name can’t have a full stop in it.");
    expect(groupNameProblem("street", fields)).toBe("A field is already called “street”. Choose another group name.");
    expect(groupNameProblem("address", fields)).toBeNull();
  });

  it("says what a field will be called", () => {
    expect(groupEffect("address", ["street"], [field("street")])).toBe(
      "Puts 1 field under one name. “street” becomes “address.street”. Answers and places stay the same.",
    );
  });
});

describe("new buttons", () => {
  it("go at the bottom right inside the margin, side by side", () => {
    const letter = { width: 612, height: 792 };
    const first = buttonPlace(letter, 0);
    const second = buttonPlace(letter, 1);
    expect(first).toEqual({ x: 612 - 36 - BUTTON_SIZE.width, y: 792 - 36 - BUTTON_SIZE.height, ...BUTTON_SIZE });
    expect(second.x + second.width).toBeLessThan(first.x);
  });
});

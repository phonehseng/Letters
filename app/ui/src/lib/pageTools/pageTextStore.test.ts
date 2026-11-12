import { describe, expect, it } from "vitest";
import { PageTextStore } from "./pageTextStore.svelte";
import type { PageText } from "./textSelection";

const empty: PageText = { chars: [], lines: [] };

/** a store whose fetches wait until the test lets them finish. */
function makeStore() {
  const asked: number[] = [];
  const waiting: (() => void)[] = [];
  const store = new PageTextStore(
    (page) =>
      new Promise((resolve) => {
        asked.push(page);
        waiting.push(() => resolve({ ...empty, lines: [{ x: page, y: 0, width: 1, height: 1 }] }));
      }),
  );
  const finish = async () => {
    waiting.splice(0).forEach((done) => done());
    await Promise.resolve();
    await Promise.resolve();
  };
  return { store, asked, finish };
}

describe("the page text store", () => {
  it("asks for each page once, and keeps the answer", async () => {
    const { store, asked, finish } = makeStore();
    store.setRevision(1);
    store.ensure([0, 1]);
    store.ensure([1, 2]);
    expect(asked).toEqual([0, 1, 2]);
    await finish();
    expect(store.get(1)?.lines[0].x).toBe(1);
    store.ensure([1]);
    expect(asked).toEqual([0, 1, 2]);
  });

  it("forgets everything when the document changes, and ignores late answers", async () => {
    const { store, asked, finish } = makeStore();
    store.setRevision(1);
    store.ensure([0]);
    store.setRevision(2);
    await finish();
    expect(store.get(0)).toBeUndefined();
    store.ensure([0]);
    expect(asked).toEqual([0, 0]);
  });
});

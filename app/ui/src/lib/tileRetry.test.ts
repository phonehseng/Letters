import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { MAX_TILE_ATTEMPTS, retryTile } from "./tileRetry";

/** just enough of an <img> for the retry logic */
function fakeImage(src: string) {
  return { src, isConnected: true, dataset: {} as Record<string, string> };
}

const failed = (image: ReturnType<typeof fakeImage>) => ({ currentTarget: image }) as unknown as Event;

describe("retryTile", () => {
  beforeEach(() => vi.useFakeTimers());
  afterEach(() => vi.useRealTimers());

  it("asks again for a tile still on screen, with a new-looking address", () => {
    const image = fakeImage("http://tile.localhost/1/0/1000/0/0");
    retryTile(failed(image));
    vi.advanceTimersByTime(250);
    expect(image.src).toBe("http://tile.localhost/1/0/1000/0/0#1");
  });

  it("gives up after a few attempts", () => {
    const image = fakeImage("http://tile.localhost/1/0/1000/0/0");
    for (let attempt = 0; attempt < MAX_TILE_ATTEMPTS + 2; attempt += 1) {
      retryTile(failed(image));
      vi.runAllTimers();
    }
    expect(image.src).toBe(`http://tile.localhost/1/0/1000/0/0#${MAX_TILE_ATTEMPTS}`);
  });

  it("leaves a tile that is no longer on screen alone", () => {
    const image = fakeImage("http://tile.localhost/1/0/1000/0/0");
    image.isConnected = false;
    retryTile(failed(image));
    vi.runAllTimers();
    expect(image.src).toBe("http://tile.localhost/1/0/1000/0/0");
  });
});

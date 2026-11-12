// when checks run: after a pause in typing (debounced), at once on save,
// and never with an answer that is out of date. each run gets a number;
// a run that finishes after a newer one started is thrown away.
// timers are passed in so the tests can drive the clock (scheduler.test.ts).

export interface Timers {
  set: (run: () => void, milliseconds: number) => unknown;
  clear: (handle: unknown) => void;
}

const realTimers: Timers = {
  set: (run, milliseconds) => setTimeout(run, milliseconds),
  clear: (handle) => clearTimeout(handle as ReturnType<typeof setTimeout>),
};

/** the in-app checks are cheap, so they follow typing closely */
export const IN_APP_PAUSE_MS = 400;

/** a compiler start costs about a quarter of a second, so it waits for a longer pause */
export const COMPILER_PAUSE_MS = 1200;

export class CheckScheduler<Result> {
  private timer: unknown = null;
  private newest = 0;

  constructor(
    private readonly run: () => Promise<Result>,
    private readonly deliver: (result: Result) => void,
    private readonly pauseMs: number,
    private readonly onStale: () => void = () => {},
    private readonly timers: Timers = realTimers,
  ) {}

  /** the text changed: check once typing has paused. */
  changed() {
    this.cancelTimer();
    this.timer = this.timers.set(() => {
      this.timer = null;
      void this.now();
    }, this.pauseMs);
  }

  /** check at once (on save, or Check Now). */
  async now(): Promise<void> {
    this.cancelTimer();
    // starting a compiler check already stops the window's older one in the
    // shell (code_commands.rs), so onStale is only needed when stopping
    const ticket = ++this.newest;
    let result: Result;
    try {
      result = await this.run();
    } catch {
      return;
    }
    // an older run that finishes late must not overwrite a newer answer
    if (ticket === this.newest) this.deliver(result);
  }

  /** the screen is closing: no pending or running check may deliver. */
  stop() {
    this.cancelTimer();
    this.newest++;
    this.onStale();
  }

  private cancelTimer() {
    if (this.timer !== null) this.timers.clear(this.timer);
    this.timer = null;
  }
}

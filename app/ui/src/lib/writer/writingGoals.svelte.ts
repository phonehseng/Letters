// the writer's quiet goals line in the status bar: the document's word goal,
// this session's words and time, and the daily streak. the numbers and the
// sentences come from the long-form work (lib/longform/goals.ts) and are
// kept on this computer by the shell (lib/reading/readingApi.ts).

import { clock, dayKey, goalLine, sessionLine, streak, wordsAdded } from "../longform/goals";
import { readingDocument, recordWords, writingLog, type WritingLog } from "../reading/readingApi";

/** how often the words written are added to today's count */
const RECORD_EVERY_MS = 60_000;

export class WritingGoals {
  log = $state<WritingLog | null>(null);
  wordGoal = $state<number | null>(null);
  sessionGoal = $state<number | null>(null);
  /** seconds since the document opened in this window */
  seconds = $state(0);
  private startWords: number | null = null;
  private recordedWords: number | null = null;
  private timer: ReturnType<typeof setInterval> | null = null;

  /** starts the timer and loads the goals for the file at `path`. */
  start(path: string, words: () => number) {
    writingLog().then((log) => (this.log = log)).catch(() => {});
    readingDocument(path)
      .then((reading) => {
        this.wordGoal = reading.wordGoal;
        this.sessionGoal = reading.sessionGoal;
      })
      .catch(() => {});
    this.startWords = words();
    this.recordedWords = this.startWords;
    let elapsed = 0;
    this.timer = setInterval(() => {
      this.seconds += 1;
      elapsed += 1000;
      if (elapsed >= RECORD_EVERY_MS) {
        elapsed = 0;
        this.record(words());
      }
    }, 1000);
  }

  /** adds the words written since the last record to today's count (deleting doesn't count against it). */
  record(words: number) {
    const added = wordsAdded(this.recordedWords ?? words, words);
    this.recordedWords = Math.max(this.recordedWords ?? words, words);
    if (added === 0) return;
    recordWords(dayKey(new Date()), added).then((log) => (this.log = log)).catch(() => {});
  }

  stop(words: number) {
    if (this.timer) clearInterval(this.timer);
    this.record(words);
  }

  /** "1,240 of 3,000 words · This session: 320 · 12:05 · 3-day streak" */
  line(words: number): string {
    const parts = [goalLine(words, this.wordGoal), sessionLine(wordsAdded(this.startWords ?? words, words), this.sessionGoal), clock(this.seconds)];
    const days = this.log ? streak(this.log.days, this.log.dailyGoal, dayKey(new Date())) : 0;
    if (days > 1) parts.push(`${days}-day streak`);
    return parts.join(" · ");
  }
}

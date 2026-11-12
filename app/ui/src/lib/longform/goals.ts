// writing goals: total and session word targets, the daily streak, the
// session timer. all quiet: a small line in the status bar, never a
// pop-up, never a nag. the numbers stay on this computer.

/** "2026-11-12" in the writer's own time zone, the key of a day. */
export function dayKey(date: Date): string {
  const month = String(date.getMonth() + 1).padStart(2, "0");
  const day = String(date.getDate()).padStart(2, "0");
  return `${date.getFullYear()}-${month}-${day}`;
}

function previousDay(key: string): string {
  const [year, month, day] = key.split("-").map(Number);
  return dayKey(new Date(year, month - 1, day - 1));
}

/**
 * days in a row that reached the daily goal, ending today, or ending
 * yesterday when today has not reached it yet (so a streak does not
 * break in the morning before any writing).
 */
export function streak(days: Record<string, number>, goal: number, today: string): number {
  const reached = (key: string) => (days[key] ?? 0) >= Math.max(goal, 1);
  let day = reached(today) ? today : previousDay(today);
  let count = 0;
  while (reached(day)) {
    count += 1;
    day = previousDay(day);
  }
  return count;
}

/** words added since the last count; deleting does not count against the day. */
export function wordsAdded(before: number, after: number): number {
  return Math.max(0, after - before);
}

/** a share from 0 to 1, for the quiet progress ring. */
export function share(done: number, goal: number | null | undefined): number | null {
  if (!goal || goal <= 0) return null;
  return Math.min(1, Math.max(0, done / goal));
}

/** "1,240 of 3,000 words" or "1,240 words" without a goal. */
export function goalLine(words: number, goal: number | null | undefined): string {
  const count = words.toLocaleString("en-US");
  return goal ? `${count} of ${goal.toLocaleString("en-US")} words` : `${count} words`;
}

/** "This session: 320 of 500" or "This session: 320". */
export function sessionLine(sessionWords: number, sessionGoal: number | null | undefined): string {
  const count = sessionWords.toLocaleString("en-US");
  return sessionGoal ? `This session: ${count} of ${sessionGoal.toLocaleString("en-US")}` : `This session: ${count}`;
}

/** "12:05" or "1:02:05" for the session timer. */
export function clock(seconds: number): string {
  const s = Math.max(0, Math.floor(seconds));
  const hours = Math.floor(s / 3600);
  const minutes = Math.floor((s % 3600) / 60);
  const rest = String(s % 60).padStart(2, "0");
  return hours > 0 ? `${hours}:${String(minutes).padStart(2, "0")}:${rest}` : `${minutes}:${rest}`;
}

/** a typed goal ("3000", "3,000", " 3 000 ") as a number, or null for none. */
export function parseGoal(typed: string): number | null {
  const digits = typed.replace(/[\s,._]/g, "");
  if (!/^\d+$/.test(digits)) return null;
  const value = Number(digits);
  return value > 0 && value < 10_000_000 ? value : null;
}

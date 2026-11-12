// what the reading aids share while a pdf is open: where the reader
// stopped, their bookmarks, Back and Forward, the printed page labels and
// the reader's speed. the viewer creates one and hands it what it needs;
// the rules themselves live in history.ts, jump.ts and progress.ts.

import { addBookmark, clearReadingPosition, pageLabels, readingDocument, recordPageTime, removeBookmark, renameBookmark, setReadingPosition, type Bookmark } from "./readingApi";
import { EMPTY_HISTORY, goBack, goForward, jumped, type History } from "./history";
import type { JumpDocument, JumpEntry } from "./jump";
import { PageTimer } from "./progress";

export interface ReadingDeps {
  path: () => string;
  documentId: () => number;
  pageCount: () => number;
  currentPage: () => number;
  goToPage: (page: number) => void;
  announce: (text: string) => void;
}

/** the position is written at most this often while scrolling, to spare the disk */
const SAVE_DELAY_MS = 800;

export class ReadingSession {
  history = $state<History>(EMPTY_HISTORY);
  bookmarks = $state<Bookmark[]>([]);
  labels = $state<string[]>([]);
  lastPosition = $state<number | null>(null);
  secondsPerPage = $state<number | null>(null);
  goToOpen = $state(false);
  outline = $state<JumpEntry[]>([]);

  private timer = new PageTimer();
  private saveTimer: ReturnType<typeof setTimeout> | null = null;

  constructor(private readonly deps: ReadingDeps) {}

  /** reads what is remembered about the document; returns where the reader left off. */
  async load(): Promise<number | null> {
    const path = this.deps.path();
    const [record, labels] = await Promise.all([readingDocument(path), pageLabels(this.deps.documentId()).catch(() => [])]);
    this.bookmarks = record.bookmarks;
    this.secondsPerPage = record.secondsPerPage;
    this.labels = labels;
    this.lastPosition = record.lastPage;
    return record.lastPage !== null && record.lastPage > 0 && record.lastPage < this.deps.pageCount() ? record.lastPage : null;
  }

  /** the page in view changed: remember it, and measure reading speed. */
  notePage(page: number) {
    const seconds = this.timer.moved(page);
    const path = this.deps.path();
    if (seconds !== null) recordPageTime(path, seconds).then((speed) => (this.secondsPerPage = speed));
    if (this.saveTimer) clearTimeout(this.saveTimer);
    this.saveTimer = setTimeout(() => setReadingPosition(path, page, 0), SAVE_DELAY_MS);
  }

  /** a jump the reader can come back from with Back. */
  jump(page: number) {
    const here = { page: this.deps.currentPage() };
    this.history = jumped(this.history, here, { page });
    this.deps.goToPage(page);
  }

  back() {
    const step = goBack(this.history, { page: this.deps.currentPage() });
    if (!step) return this.deps.announce("Nothing to go back to.");
    this.history = step.history;
    this.deps.goToPage(step.to.page);
    this.deps.announce(`Back to page ${step.to.page + 1}.`);
  }

  forward() {
    const step = goForward(this.history, { page: this.deps.currentPage() });
    if (!step) return this.deps.announce("Nothing to go forward to.");
    this.history = step.history;
    this.deps.goToPage(step.to.page);
    this.deps.announce(`Forward to page ${step.to.page + 1}.`);
  }

  /** "Start from the Beginning": page 1, and the remembered place is forgotten. */
  async startFromBeginning() {
    this.jump(0);
    this.lastPosition = null;
    await clearReadingPosition(this.deps.path());
  }

  async addBookmark(name = "") {
    const page = this.deps.currentPage();
    const made = await addBookmark(this.deps.path(), name, page);
    this.bookmarks = [...this.bookmarks, made].sort((a, b) => a.page - b.page);
    this.deps.announce(`Bookmark added: ${made.name}.`);
    return made;
  }

  async renameBookmark(id: string, name: string) {
    await renameBookmark(this.deps.path(), id, name);
    this.bookmarks = this.bookmarks.map((b) => (b.id === id ? { ...b, name: name.trim() || b.name } : b));
  }

  async removeBookmark(id: string) {
    await removeBookmark(this.deps.path(), id);
    this.bookmarks = this.bookmarks.filter((b) => b.id !== id);
    this.deps.announce("Bookmark removed.");
  }

  /** what the page finder knows about this document. */
  jumpDocument(): JumpDocument {
    return {
      pageCount: this.deps.pageCount(),
      currentPage: this.deps.currentPage(),
      pageLabels: this.labels,
      outline: this.outline,
      bookmarks: this.bookmarks.map((b) => ({ title: b.name, page: b.page, level: 0 })),
      lastPosition: this.lastPosition,
      wordStarts: [],
    };
  }

  /** the window was left: time away is not reading time. */
  pauseTiming() {
    this.timer.restart();
  }
}

// what the Font Library and every font picker in a window share: the
// families, the person's favourites and recent fonts, and the library's
// progress (the first scan of system fonts, downloads). it follows the
// shell by asking for the status while something is under way.

import { fontsApi, type FamilySummary, type LibraryStatus, type Personal, type WritingChoice } from "./fontsApi";

/** how often to ask while a scan or a download runs. */
const POLL_MS = 400;

export class FontStore {
  families = $state<FamilySummary[]>([]);
  /** the revision `families` belongs to; search answers carry theirs */
  revision = $state(-1);
  status = $state<LibraryStatus | null>(null);
  personal = $state<Personal>({ favourites: [], recent: [] });
  writing = $state<WritingChoice[]>([]);
  error = $state<string | null>(null);

  private timer: ReturnType<typeof setTimeout> | null = null;
  private users = 0;
  private loadingFamilies: Promise<void> | null = null;
  private personalLoaded = false;

  /** a screen that shows fonts starts following; call the result to stop. */
  follow(): () => void {
    this.users += 1;
    if (this.users === 1) void this.tick();
    return () => {
      this.users -= 1;
      if (this.users === 0 && this.timer) {
        clearTimeout(this.timer);
        this.timer = null;
      }
    };
  }

  get scanning(): boolean {
    return this.status !== null && !this.status.scan.finished;
  }

  private busy(status: LibraryStatus): boolean {
    return !status.scan.finished || status.downloads.some((download) => !download.finished && !download.error);
  }

  private async tick() {
    this.timer = null;
    try {
      const status = await fontsApi.status();
      this.status = status;
      if (status.revision !== this.revision) await this.loadFamilies();
      if (!this.writing.length) this.writing = await fontsApi.writingSystems();
      if (!this.personalLoaded) {
        this.personal = await fontsApi.personal();
        this.personalLoaded = true;
      }
      if (this.users > 0) this.timer = setTimeout(() => this.tick(), this.busy(status) ? POLL_MS : POLL_MS * 10);
    } catch (problem) {
      this.error = String(problem);
    }
  }

  /** reads the families again (after a change), once at a time. */
  loadFamilies(): Promise<void> {
    if (!this.loadingFamilies) {
      this.loadingFamilies = fontsApi
        .families()
        .then((answer) => {
          this.families = answer.families;
          this.revision = answer.revision;
        })
        .finally(() => {
          this.loadingFamilies = null;
        });
    }
    return this.loadingFamilies;
  }

  /** asks for the status now, after starting something (a download). */
  async refresh() {
    if (this.timer) clearTimeout(this.timer);
    await this.tick();
  }

  family(name: string): FamilySummary | undefined {
    const key = name.trim().toLowerCase();
    return this.families.find((family) => family.name.toLowerCase() === key);
  }

  async setFavourite(name: string, favourite: boolean) {
    this.personal = await fontsApi.setFavourite(name, favourite);
    this.families = this.families.map((family) => (family.name === name ? { ...family, favourite } : family));
  }

  async noteUsed(name: string) {
    this.personal = await fontsApi.noteUsed(name);
  }
}

let shared: FontStore | null = null;

/** the one store of this window. */
export function fontStore(): FontStore {
  shared ??= new FontStore();
  return shared;
}

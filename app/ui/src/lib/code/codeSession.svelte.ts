// one code file while its window shows it: its path and name, whether it
// has unsaved changes, saving (with Save As and the warning for text that
// wasn't valid), and recovery copies. the editor's text lives in
// codemirror; this class only keeps the last saved version to compare.

import type { Text } from "@codemirror/state";
import { asCommandError } from "../api";
import { autosaveCode, chooseCodeSaveLocation, discardCodeRecovery, saveCodeFile, type CodeDocument } from "./codeApi";
import { languageForPath, languageInfo, type LanguageId } from "./languages";
import { FULL_FEATURES_LIMIT_BYTES, type TextFormat } from "./textFormat";

/** recovery copies are written this often while there are unsaved changes, as the writer does */
export const AUTOSAVE_INTERVAL_MS = 5000;

// sending a very large text every few seconds would cost more than it saves
const AUTOSAVE_LIMIT_BYTES = 8 * 1024 * 1024;

export interface Notice {
  text: string;
  action?: { label: string; run: () => void };
}

export class CodeSession {
  path = $state<string | null>(null);
  name = $state("Untitled");
  format = $state<TextFormat>({ encoding: "utf8", byteOrderMark: false, lineEnding: "lf", mixedLineEndings: false, valid: true });
  dirty = $state(false);
  notice = $state<Notice | null>(null);
  readonly readOnly: boolean;
  readonly fullFeatures: boolean;
  private savedDoc: Text | null = null;
  private currentDoc: Text | null = null;
  private autosavedDoc: Text | null = null;

  constructor(
    document: CodeDocument,
    private readonly onError: (message: string) => void,
  ) {
    this.path = document.path;
    this.name = document.name;
    this.format = document.format;
    this.readOnly = document.readOnly;
    this.fullFeatures = document.fullFeatures && document.bytes <= FULL_FEATURES_LIMIT_BYTES;
  }

  /** the language a file's name says, or the guess an untitled block came with. */
  startingLanguage(document: CodeDocument): LanguageId {
    return document.language ?? (document.path ? languageForPath(document.path) : null) ?? languageForPath(document.name) ?? "text";
  }

  /** the editor's first text counts as saved, unless it is a recovered or new block. */
  started(doc: Text, savedOnDisk: boolean) {
    this.currentDoc = doc;
    this.savedDoc = savedOnDisk ? doc : null;
    this.dirty = !savedOnDisk;
  }

  changed(doc: Text) {
    this.currentDoc = doc;
    // comparing a large text on every key would cost more than it tells
    this.dirty = this.savedDoc === null || !this.fullFeatures || !doc.eq(this.savedDoc);
  }

  /** saves to the file, or asks where for an untitled block. false when not saved. */
  async save(): Promise<boolean> {
    if (!this.path) return this.saveAs();
    if (!this.format.valid) {
      this.askBeforeSavingInvalidText();
      return false;
    }
    return this.writeTo(this.path);
  }

  async saveAs(): Promise<boolean> {
    const extension = this.name.includes(".") ? (this.name.split(".").pop() ?? "") : "";
    const language = languageForPath(this.name);
    const label = language ? `${languageInfo(language).label} file` : "Text file";
    const chosen = await chooseCodeSaveLocation(this.path ?? this.name, extension, label);
    if (!chosen) return false;
    const saved = await this.writeTo(chosen);
    if (saved) {
      this.path = chosen;
      this.name = chosen.split(/[\\/]/).pop() ?? chosen;
    }
    return saved;
  }

  private askBeforeSavingInvalidText() {
    this.notice = {
      text: "Some bytes in this file weren’t valid text and show as “�”. Saving replaces them for good. To keep the file as it was, choose Save As instead.",
      action: {
        label: "Save Anyway",
        run: () => {
          this.format = { ...this.format, valid: true };
          void this.save();
        },
      },
    };
  }

  private async writeTo(path: string): Promise<boolean> {
    const doc = this.currentDoc;
    if (!doc) return false;
    try {
      await saveCodeFile(path, doc.toString(), this.format);
      this.savedDoc = doc;
      this.dirty = this.currentDoc !== null && this.currentDoc !== doc && !this.currentDoc.eq(doc);
      return true;
    } catch (problem) {
      this.onError(errorText(problem));
      return false;
    }
  }

  /** a recovery copy, when there are changes the last copy doesn't have. */
  async autosave() {
    const doc = this.currentDoc;
    if (!this.path || !this.dirty || !doc || doc === this.autosavedDoc || doc.length > AUTOSAVE_LIMIT_BYTES) return;
    this.autosavedDoc = doc;
    await autosaveCode(this.path, doc.toString()).catch(() => {});
  }

  async discardRecovery() {
    if (this.path) await discardCodeRecovery(this.path).catch(() => {});
  }
}

/** the shell's sentence for a failed command. */
export function errorText(problem: unknown): string {
  return asCommandError(problem).message;
}

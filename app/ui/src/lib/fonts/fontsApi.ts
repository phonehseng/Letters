// every call from the font library and the font picker into the shell,
// typed. the shapes mirror core/fonts (serde camelCase).

import { invoke } from "@tauri-apps/api/core";

export type Source = "system" | "custom" | "bundled" | "library" | "downloadable";
export type Category = "serif" | "sansSerif" | "monospace" | "display" | "handwriting" | "symbol";
export type Place = "onThisComputer" | "addedByMe" | "lettersLibrary" | "toDownload";

/** one family as the lists show it. */
export interface FamilySummary {
  name: string;
  category: Category;
  sources: Source[];
  onThisComputer: boolean;
  /** the web view must be given the font's bytes to draw it (not installed on the system) */
  needsFontFace: boolean;
  regularFace: string | null;
  styleCount: number;
  hasBold: boolean;
  hasItalic: boolean;
  variable: boolean;
  monospace: boolean;
  embeddable: boolean;
  favourite: boolean;
  downloadBytes: number | null;
}

export interface Style {
  name: string;
  weight: number;
  italic: boolean;
  /** the face that draws it, when it is on this computer */
  face: string | null;
}

export interface Axis {
  tag: string;
  min: number;
  default: number;
  max: number;
}

export interface Similar {
  family: string;
  metricCompatible: boolean;
  distance: number;
}

export interface FamilyDetails {
  summary: FamilySummary;
  styles: Style[];
  writing: string[];
  embeddingSentence: string;
  license: string | null;
  sourceSentence: string;
  designer: string | null;
  version: string | null;
  glyphCount: number | null;
  axes: Axis[];
  files: string[];
  similar: Similar[];
  previewReady: boolean;
}

export interface SearchQuery {
  text?: string;
  categories?: Category[];
  writing?: string[];
  places?: Place[];
  monospace?: boolean;
  hasItalic?: boolean;
  hasBold?: boolean;
  variable?: boolean;
  minStyles?: number;
  embeddable?: boolean;
  sample?: string;
  similarTo?: string;
  favouritesOnly?: boolean;
}

export interface SearchResults {
  /** positions in the family list, best first */
  families: number[];
  /** how typed phrases were read: "Supports Greek", "Similar to Arial" */
  understood: string[];
  metricCompatible: number[];
}

export interface DownloadState {
  family: string;
  progress: { done: number; total: number };
  finished: boolean;
  error: string | null;
}

export interface LibraryStatus {
  revision: number;
  scan: { done: number; total: number; finished: boolean };
  familyCount: number;
  downloads: DownloadState[];
  downloadNoticeAccepted: boolean;
  downloadHost: string;
}

export interface Personal {
  favourites: string[];
  recent: string[];
}

export interface Preview {
  width: number;
  height: number;
  baseline: number;
  path: string;
  missing: string[];
}

export interface Rejected {
  file: string;
  reason: string;
}

export interface AddReport {
  added: unknown[];
  rejected: Rejected[];
  alreadyAdded: string[];
}

export interface Substitution {
  asked: string;
  used: string;
  metricCompatible: boolean;
  available: boolean;
  downloadable: boolean;
  sentence: string;
}

export interface MissingFont {
  name: string;
  downloadable: boolean;
  substitution: Substitution;
}

export interface WritingChoice {
  id: string;
  name: string;
}

export const fontsApi = {
  status: () => invoke<LibraryStatus>("fonts_status"),
  families: () => invoke<{ revision: number; families: FamilySummary[] }>("fonts_families"),
  search: (query: SearchQuery) => invoke<{ revision: number; results: SearchResults }>("fonts_search", { query }),
  details: (name: string) => invoke<FamilyDetails>("fonts_details", { name }),
  writingSystems: () => invoke<WritingChoice[]>("fonts_writing_systems"),
  /** a face as a plain font, for FontFace (raw bytes, not json) */
  faceBytes: (key: string) => invoke<ArrayBuffer>("fonts_face_bytes", { key }),
  preview: (name: string, text: string, size: number, weight: number | null = null, italic = false) =>
    invoke<Preview>("fonts_preview", { name, text, size, weight, italic }),
  add: (paths: string[]) => invoke<AddReport>("fonts_add", { paths }),
  remove: (name: string) => invoke<void>("fonts_remove", { name }),
  setFavourite: (name: string, favourite: boolean) => invoke<Personal>("fonts_set_favourite", { name, favourite }),
  noteUsed: (name: string) => invoke<Personal>("fonts_note_used", { name }),
  personal: () => invoke<Personal>("fonts_personal"),
  acceptDownloadNotice: () => invoke<void>("fonts_accept_download_notice"),
  download: (name: string) => invoke<void>("fonts_download", { name }),
  cancelDownload: (name: string) => invoke<void>("fonts_cancel_download", { name }),
  fetchPreview: (name: string) => invoke<void>("fonts_fetch_preview", { name }),
  missing: (names: string[]) => invoke<MissingFont[]>("fonts_missing", { names }),
  similar: (name: string) => invoke<Similar[]>("fonts_similar", { name }),
};

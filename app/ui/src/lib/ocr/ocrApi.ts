// every call from the Scan tab and New from Images into the shell,
// typed. the shapes mirror core/ocr (letters-ocr) and shell-core's ocr
// module, which name fields in camelCase. pages count from 0.

import { invoke } from "@tauri-apps/api/core";
import type { DocumentState } from "../api";
import type { Rect } from "../pageTools/pagePoints";

export interface OcrLanguage {
  /** tesseract's code, such as "eng" */
  code: string;
  /** in plain english, such as "English" */
  name: string;
}

export interface OcrStatus {
  available: boolean;
  /** why it isn't, and what to do */
  message: string | null;
  languages: OcrLanguage[];
  canDetectOrientation: boolean;
}

export interface ScanOverview {
  pageCount: number;
  scanned: number[];
  recognized: number[];
  withLayer: number[];
}

/** the cleanup steps; the rest stay on (see core/ocr's PreprocessOptions) */
export interface CleanupChoices {
  /** turn sideways and upside-down pages upright */
  orientation: boolean;
  /** straighten slightly tilted pages */
  deskew: boolean;
  /** remove specks of dust and scanner noise */
  denoise: boolean;
}

export interface OcrOptions {
  languages: string[];
  preprocess: CleanupChoices & { contrast: boolean; binarize: boolean; cleanBorders: boolean };
}

export interface JobProgress {
  done: number;
  total: number;
  current: number | null;
}

export type JobState = "running" | "cancelling" | "finished" | "cancelled" | "failed";

export interface OcrSummary {
  pages: number[];
  pagesWithText: number;
  blankPages: number[];
  meanConfidence: number;
  lowConfidencePages: { page: number; confidence: number }[];
  wordCount: number;
}

export interface JobStatus {
  id: number;
  document: number;
  state: JobState;
  progress: JobProgress;
  error: string | null;
  summary: OcrSummary | null;
}

export interface OverlayWord {
  index: number;
  text: string;
  rect: Rect;
  /** 0 to 100, when tesseract's own words are known */
  confidence: number | null;
}

export interface OverlayPage {
  page: number;
  words: OverlayWord[];
  withConfidence: boolean;
}

export type PaperSize = "fromImage" | "a4" | "letter";

export interface ImagesStatus {
  progress: JobProgress;
  finished: boolean;
  error: string | null;
}

export function optionsFrom(languages: string[], cleanup: CleanupChoices): OcrOptions {
  return { languages, preprocess: { ...cleanup, contrast: true, binarize: true, cleanBorders: true } };
}

export const ocrApi = {
  status: () => invoke<OcrStatus>("ocr_status"),
  overview: (id: number) => invoke<ScanOverview>("ocr_scan_overview", { id }),
  start: (id: number, pages: number[], options: OcrOptions) => invoke<number>("ocr_start", { id, pages, options }),
  jobStatus: (job: number) => invoke<JobStatus>("ocr_job_status", { job }),
  cancel: (job: number) => invoke<boolean>("ocr_cancel", { job }),
  discard: (job: number) => invoke<void>("ocr_discard", { job }),
  apply: (id: number, job: number) => invoke<{ document: DocumentState; summary: OcrSummary }>("ocr_apply", { id, job }),
  removeText: (id: number, pages: number[]) => invoke<DocumentState>("ocr_remove_text", { id, pages }),
  overlay: (id: number, page: number) => invoke<OverlayPage>("ocr_overlay", { id, page }),
  correctWord: (id: number, page: number, index: number, text: string) =>
    invoke<{ document: DocumentState; overlay: OverlayPage }>("ocr_correct_word", { id, page, index, text }),
  imagesToPdf: (paths: string[], paper: PaperSize, readText: OcrOptions | null) =>
    invoke<number>("ocr_images_to_pdf", { paths, paper, readText }),
  imagesStatus: (job: number) => invoke<ImagesStatus>("ocr_images_status", { job }),
  imagesCancel: (job: number) => invoke<void>("ocr_images_cancel", { job }),
  openNewDocument: (job: number) => invoke<DocumentState>("ocr_open_new_document", { job }),
};

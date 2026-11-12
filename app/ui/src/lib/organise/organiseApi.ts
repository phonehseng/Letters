// every call into the shell for organising documents, typed. moving
// changes where a file lives on this computer; nothing is uploaded.

import { invoke } from "@tauri-apps/api/core";

export type Conflict = "ask" | "keepBoth" | "replace" | "skip";

export type Done = { kind: "moved" | "copied" | "renamed"; from: string; to: string };

export interface OrganiseReport {
  done: Done[];
  skipped: string[];
  conflict: { path: string; existing: string; remaining: string[] } | null;
  problem: string | null;
}

export const moveDocuments = (paths: string[], folder: string, conflict: Conflict = "ask"): Promise<OrganiseReport> => invoke("organise_move", { paths, folder, conflict });

export const copyDocuments = (paths: string[], folder: string, conflict: Conflict = "ask"): Promise<OrganiseReport> => invoke("organise_copy", { paths, folder, conflict });

export const renameDocument = (path: string, name: string): Promise<OrganiseReport> => invoke("organise_rename", { path, name });

export const undoOrganise = (done: Done[]): Promise<OrganiseReport> => invoke("organise_undo", { done });

export const recycleDocuments = (paths: string[]): Promise<number> => invoke("organise_recycle", { paths });

export const revealDocument = (path: string): Promise<void> => invoke("organise_reveal", { path });

export const newFolder = (parent: string, name?: string): Promise<string> => invoke("organise_new_folder", { parent, name: name ?? null });

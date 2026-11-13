// the writer's ways out of the editor: Save As (any of the engine's
// formats), Export as PDF and Print. the screen calls these; they talk to
// the shell through the session and writerApi.

import { copySavedSentence, formatForPath, saveFilters } from "./formats";
import { openPrintDialog, printPdfFile } from "./print";
import * as api from "./writerApi";
import { errorText, type WriterSession } from "./writerSession.svelte";

/** how long the notice waits before the surface prints itself, so the person reads why */
const SURFACE_PRINT_DELAY_MS = 300;

/**
 * Save As: Word and OpenDocument become the document's file, the others are
 * a copy. returns true when saved; asks `askOdt` first when .odt would lose
 * something (that question then saves on its own).
 */
export async function saveAs(session: WriterSession, onError: (message: string) => void, askOdt: (question: { text: string; path: string }) => void): Promise<boolean> {
  const formats = await session.loadFormats();
  const path = await api.chooseSaveLocationWithFilters(session.path, saveFilters(session.kind, formats));
  if (!path) return false;
  const format = formatForPath(path, formats);
  if (!format) {
    const offered = formats.filter((each) => each.available).map((each) => each.label);
    onError(`Letters can’t save in that format. Choose ${offered.join(", ")}.`);
    return false;
  }
  if (format.id === "pdf") {
    await session.exportPdf(path);
    return true;
  }
  if (format.becomes === "copy") return saveCopy(session, onError, path, format);
  if (/\.odt$/i.test(path)) {
    const losses = await api.odtLosses(session.id, session.model()).catch(() => null);
    if (losses) {
      askOdt({ text: losses, path });
      return false;
    }
  }
  return session.save(path);
}

async function saveCopy(session: WriterSession, onError: (message: string) => void, path: string, format: NonNullable<ReturnType<typeof formatForPath>>): Promise<boolean> {
  try {
    await api.exportCopy(session.id, session.model(), path);
    session.say(`${copySavedSentence(format, session.name)} ${format.note}`);
    return true;
  } catch (problem) {
    onError(errorText(problem));
    return false;
  }
}

/** Export as PDF: asks where, beside the document by default. */
export async function exportPdf(session: WriterSession) {
  const path = await api.chooseSaveLocationFor(api.withExtension(session.path, "pdf"), "pdf");
  if (path) await session.exportPdf(path);
}

/**
 * prints Word's print-ready pdf (exact), or LibreOffice's (close, and the
 * notice says so), or without either the surface itself. the system's
 * print dialog gets the pdf's own page sizes.
 */
export async function printDocument(session: WriterSession) {
  session.say("Preparing to print… Word can take a few seconds.");
  try {
    const made = await api.printReadyPdf(session.id, session.model());
    if (made.exact) session.notice = null;
    else session.say(made.note ?? "Printed through LibreOffice. Line and page breaks can differ slightly from Word.");
    await printPdfFile(made.path);
  } catch {
    session.say("Word and LibreOffice aren’t installed, so Letters prints its own pages. Page breaks may differ from Word’s.");
    // through the shell: a mac's web view ignores window.print()
    setTimeout(() => void openPrintDialog(), SURFACE_PRINT_DELAY_MS);
  }
}

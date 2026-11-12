// opens the Font Library over the window from anywhere: the font
// picker's More Fonts…, a menu command or a shortcut. one library at a
// time; opening it again shows the same one.

import { mount, unmount } from "svelte";
import FontLibrary from "./FontLibrary.svelte";

export interface FontLibraryOptions {
  /** fonts the open document uses, for the Missing Fonts view */
  documentFonts?: string[];
  /** replaces a font in the open document; Replace is offered only with it */
  onReplaceFont?: (from: string, to: string) => void;
  /** a family to show first */
  family?: string;
  /** open on the Missing Fonts view */
  view?: "all" | "missing";
  /** called after the library closes */
  onClosed?: () => void;
}

let open: { instance: ReturnType<typeof mount>; host: HTMLElement; restore: Element | null } | null = null;

/** what the open document says it uses, for when the library opens by key. */
let documentContext: Pick<FontLibraryOptions, "documentFonts" | "onReplaceFont"> = {};

/**
 * a screen with a document tells the library which fonts it uses and
 * how to replace one. returns a function that takes it back.
 */
export function setDocumentFonts(context: Pick<FontLibraryOptions, "documentFonts" | "onReplaceFont">): () => void {
  documentContext = context;
  return () => {
    if (documentContext === context) documentContext = {};
  };
}

export function isFontLibraryOpen(): boolean {
  return open !== null;
}

export function closeFontLibrary() {
  if (!open) return;
  const { instance, host, restore } = open;
  open = null;
  void unmount(instance);
  host.remove();
  (restore as HTMLElement | null)?.focus?.();
}

export function openFontLibrary(options: FontLibraryOptions = {}) {
  if (open) return;
  const host = document.createElement("div");
  host.className = "font-library-host";
  document.body.append(host);
  const restore = document.activeElement;
  const instance = mount(FontLibrary, {
    target: host,
    props: {
      documentFonts: options.documentFonts ?? documentContext.documentFonts ?? [],
      onReplaceFont: options.onReplaceFont ?? documentContext.onReplaceFont,
      initialFamily: options.family,
      initialView: options.view ?? "all",
      onClose: () => {
        closeFontLibrary();
        options.onClosed?.();
      },
    },
  });
  open = { instance, host, restore };
}

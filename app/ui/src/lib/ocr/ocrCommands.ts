// the Scan tab's commands, for the menu bar (the Content menu and File >
// New from Images…) and the keyboard. commands.ts adds them to the app's
// one list. New from Images is the app's own command (App.svelte), since
// it works on the Recents screen too.

export const OCR_COMMANDS = ["make-searchable", "show-recognized-text", "remove-recognized-text"] as const;

export type OcrCommand = (typeof OCR_COMMANDS)[number];

export const NEW_FROM_IMAGES = "new-from-images" as const;

/** ctrl and alt with O (control and command on a mac): O for OCR. */
export const OCR_KEYS: Record<string, OcrCommand> = {
  KeyO: "make-searchable",
};

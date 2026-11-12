// the writer's commands, by name. they join the app's one command list in
// ../commands.ts, so the macOS menu bar (Format and Insert menus) and the
// writer's keyboard shortcuts reach them through the same dispatcher.

export const WRITER_COMMANDS = [
  "format-bold",
  "format-italic",
  "format-underline",
  "format-strike",
  "format-superscript",
  "format-subscript",
  "style-title",
  "style-heading-1",
  "style-heading-2",
  "style-heading-3",
  "style-body",
  "style-quote",
  "align-left",
  "align-center",
  "align-right",
  "align-justify",
  "list-bullets",
  "list-numbers",
  "indent-more",
  "indent-less",
  "insert-table",
  "insert-picture",
  "insert-page-break",
  "insert-footnote",
  "insert-link",
  "insert-page-number",
  "insert-comment",
  "find-replace",
  "track-changes",
  "export-pdf",
  "toggle-ruler",
  "print",
  "save-to-drive",
  "toggle-strict-mode",
  "image-inline",
  "image-free",
  // second round: text blocks, arranging objects, cropping, word's layout
  "text-block-split",
  "text-block-make",
  "text-block-dissolve",
  "object-front",
  "object-back",
  "image-crop",
  "check-layout",
  // the long-form work's Focus Mode (lib/longform)
  "toggle-focus-mode",
] as const;

export type WriterCommand = (typeof WRITER_COMMANDS)[number];

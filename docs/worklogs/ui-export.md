# Worklog: app/ui/src/lib/export

## 2026-11-11 the Export sheet

Added `exportChoices.ts` (capabilities to the format list, options, checking,
the request; tested), `ExportSheet.svelte` and `exportApi.ts`. Exact Match is
the default for Word, as the brief asks; unavailable formats stay in the list
greyed with their reason so the list never changes shape.

## 2026-11-11 commands, words and opening by kind

Added `conversionCommands.ts` and `conversionTools.ts` (joined into
`commands.ts` and `tools.ts`, the one list of commands and words) for Export
To…, Open PDF as Document…, Settings…, AI Tools and Find a Topic, and
`openByKind.ts` for spreadsheets and presentations shown as pdfs.

## 2026-11-12 exact match details

The result shows "How close is it?" with the engine's measured numbers and
each font stand-in, and the hint before exporting now uses the shell's own
answer about Word, because a separate check could arrive late and say the
result can't be checked when it can.

## 2026-11-12 Convert File…

Asked for: a converter reachable from the menu or the top bar, not only
from inside an open file. `ConvertSheet.svelte` chooses a file (or starts
with the one the window shows), lists the types the shell offers for it,
and asks where the copy goes, suggesting the same name beside the
original. It is on the Recents toolbar and as a "Convert a File…" tile
beside New from Images…, in the PDF viewer's document menu, the writer's
document menu, and the mac's File menu. A PDF chosen there opens with
Export To…, and a picture with New from Images…. The result offers Show
in Folder and Open.

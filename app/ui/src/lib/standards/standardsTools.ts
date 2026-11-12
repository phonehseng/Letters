// the plain-english description of Prepare & Check and each of its
// tools, for the document menu's tooltip and Explain. tools.ts lists these
// with every other tool.

import type { ToolHelp } from "../tools";

const NEW_FILE = "This document isn’t changed: the result is a new file, saved where you choose.";

export const standardsTools = {
  prepareAndCheck: {
    label: "Prepare & Check…",
    tooltip: "Check this PDF, make an archival or smaller copy, or check it for printing",
    whenToUse:
      "Before handing a PDF in or sending it to print: a university archive asking for PDF/A, a portal with a size limit, a print shop asking for PDF/X, or a PDF that looks different on another computer.",
    fileEffect: `Checks only read the document. Converting to PDF/A and making a smaller copy save new files. ${NEW_FILE}`,
    meaning:
      "PDF/A is the kind of PDF archives keep for good: everything it needs to look the same is inside it. PDF/X is the kind print shops ask for. Letters checks with its own checker, and says so with every result.",
  },
  checkPdfa: {
    label: "PDF/A",
    tooltip: "Check this PDF against an archival standard, and convert a copy",
    whenToUse: "When a library, archive or university asks for PDF/A, usually 2b. Check first to see what would change.",
    fileEffect: `Check reads the document. Convert and Save a Copy… fixes what Letters can and saves the copy. ${NEW_FILE}`,
    meaning: "PDF/A keeps a document readable for decades: fonts embedded, colours defined, and nothing that depends on another file or a script.",
  },
  preflightPdf: {
    label: "Check This PDF",
    tooltip: "Look for problems that make a PDF look or print differently elsewhere",
    whenToUse: "Before sending a PDF to someone else: missing fonts, blurry pictures, damaged parts of the file, and features some apps don’t show.",
    fileEffect: "Nothing. It only reads the file.",
  },
  optimizePdf: {
    label: "Make Smaller",
    tooltip: "Save a smaller copy that looks the same",
    whenToUse: "When a portal or an email refuses a PDF as too big.",
    fileEffect: `Saves a smaller copy, removing unused parts, storing repeated parts once, compressing, and if you choose, making very sharp pictures less sharp. ${NEW_FILE} Says what it didn’t do.`,
  },
  checkPdfx: {
    label: "Print Check",
    tooltip: "Check this PDF against a print standard (PDF/X)",
    whenToUse: "Before sending a poster or a booklet to a print shop that asks for PDF/X.",
    fileEffect: "Nothing. It only reports: Letters doesn’t convert to PDF/X, because that needs the print shop’s colour profile.",
  },
} satisfies Record<string, ToolHelp>;

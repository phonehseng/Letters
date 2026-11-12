// the words of conversion and export, the AI Tools and Settings: label,
// tooltip, shortcut and the Explain entry of each, joined into tools.ts.
// wording rule: the feature is called "AI Tools"; no company, service or
// model is named anywhere, and examples use made-up names.

import type { ToolHelp } from "../tools";

const NOTHING = "Nothing. The file is not changed.";
const NEW_FILE = "This document and its file on disk don’t change; a new file is made where you choose.";

export const conversionTools = {
  exportTo: {
    label: "Export To…",
    tooltip: "Save a copy as Word, Excel, PowerPoint, pictures, a web page or text",
    shortcut: "Ctrl Shift E",
    whenToUse: "To edit a PDF’s text in Word, put its tables in a spreadsheet, show its pages as slides, or send pictures of its pages.",
    fileEffect: NEW_FILE,
    meaning: "Exporting makes a copy in another format; the PDF itself stays as it is.",
  },
  openPdfAsDocument: {
    label: "Open PDF as Document…",
    tooltip: "Open a PDF’s text as a document you can edit, in a new window",
    whenToUse: "To change the words of a PDF as if it were an essay: headings, paragraphs, lists and tables flow as you type.",
    fileEffect: "The PDF doesn’t change. The document is new and unsaved; Save puts it beside the PDF as a Word document.",
  },
  convertToPdf: {
    label: "Convert to PDF…",
    tooltip: "Make a PDF of this file with Microsoft Office or LibreOffice",
    whenToUse: "For spreadsheets, presentations and Rich Text files, which Letters shows and shares as PDFs.",
    fileEffect: NEW_FILE,
  },
  settings: {
    label: "Settings…",
    tooltip: "Change how Letters works",
    shortcut: "Ctrl ,",
    whenToUse: "To change your name on comments, the default zoom, where files are saved, AI Tools, or to make Letters the default app.",
    fileEffect: NOTHING,
  },
  aiTools: {
    label: "AI Tools",
    tooltip: "Summarize, list key points or ask questions about this document",
    whenToUse: "To get the gist of a long reading, check what it says about something, or find where it says it. Answers quote the document and link to the page.",
    fileEffect: NOTHING,
    meaning: "AI Tools use a language model you set up in Settings. A model on this computer keeps everything here; an online service gets the text it needs, and Letters asks before sending it.",
  },
  findTopic: {
    label: "Find a Topic…",
    tooltip: "Find the passages about something, even when the words differ a little",
    shortcut: "Ctrl Shift F",
    whenToUse: "When Find needs the exact words but you only know the subject, such as “photosynthesis” or “the causes of the war”.",
    fileEffect: NOTHING,
    meaning: "Find a Topic works on this computer with no AI: it scores each passage by the words it shares with yours, word endings included.",
  },
  aiSummarize: {
    label: "Summarize",
    tooltip: "A short, medium or detailed summary of the pages you choose",
    whenToUse: "Before reading closely, to know what a chapter is about, or after, to check you got the main ideas.",
    fileEffect: NOTHING,
  },
  aiKeyPoints: {
    label: "Key Points",
    tooltip: "The main points as a list, each with its pages",
    whenToUse: "To make revision notes or check a reading’s main claims.",
    fileEffect: NOTHING,
  },
  aiAsk: {
    label: "Ask",
    tooltip: "Ask a question; the answer quotes the document",
    whenToUse: "To check what the document says about something. Click a quote to see it on the page. If the document doesn’t say, the answer says so.",
    fileEffect: NOTHING,
  },
  makeDefaultApp: {
    label: "Make Letters the Default App…",
    tooltip: "Choose Letters for PDFs and documents in the system’s settings",
    whenToUse: "So double clicking a PDF or a Word document opens it in Letters.",
    fileEffect: "Nothing in your files. The system remembers which app opens each kind of file.",
  },
} satisfies Record<string, ToolHelp>;

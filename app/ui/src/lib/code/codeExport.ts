// the formats "Export as Code File…" offers, what each is for, and which
// kind of document can make it. the same names as CodeExportFormat in
// app/shell-core/src/code/export/mod.rs.

export type CodeExportFormat = "markdown" | "html" | "plainText" | "latex" | "restructuredText" | "asciiDoc" | "json" | "csv";

export interface CodeExportChoice {
  format: CodeExportFormat;
  label: string;
  extension: string;
  /** one sentence: what the format is and when to use it */
  summary: string;
}

export const CODE_EXPORT_CHOICES: CodeExportChoice[] = [
  { format: "markdown", label: "Markdown", extension: "md", summary: "Plain text with # headings and **bold**. Reads well on GitHub and in notes apps." },
  { format: "html", label: "Web Page (HTML)", extension: "html", summary: "A web page with headings, lists and tables, ready to style with CSS." },
  { format: "plainText", label: "Plain Text", extension: "txt", summary: "Just the words, one paragraph per line." },
  { format: "latex", label: "LaTeX", extension: "tex", summary: "A basic LaTeX article with sections, lists, emphasis, tables and pictures. Compile it with pdflatex or Overleaf." },
  { format: "restructuredText", label: "reStructuredText", extension: "rst", summary: "The format of Python’s documentation and Sphinx." },
  { format: "asciiDoc", label: "AsciiDoc", extension: "adoc", summary: "A plain-text format for books and technical documentation." },
  { format: "json", label: "JSON Outline", extension: "json", summary: "The document’s headings, paragraphs, lists and tables as data, for your own programs." },
  { format: "csv", label: "CSV Tables", extension: "csv", summary: "Each table as a spreadsheet file. A second table gets its own file beside the first." },
];

// formats people ask for that no engine here writes yet; listed greyed so
// the list doesn't change shape when they arrive (DESIGN.md, Export To…)
export const CODE_EXPORT_LATER: { label: string; reason: string }[] = [
  { label: "Jupyter Notebook (.ipynb)", reason: "Coming later." },
  { label: "Org Mode (.org)", reason: "Coming later." },
];

export function choiceFor(format: CodeExportFormat): CodeExportChoice {
  return CODE_EXPORT_CHOICES.find((choice) => choice.format === format) ?? CODE_EXPORT_CHOICES[0];
}

/** "Essay.docx" becomes "Essay.tex" in the same folder. */
export function suggestedExportPath(sourcePath: string, format: CodeExportFormat): string {
  return sourcePath.replace(/\.[^.\\/]+$/, "") + `.${choiceFor(format).extension}`;
}

/** the line under the Export button: exactly what will happen. */
export function exportLine(format: CodeExportFormat, sourceKind: "writer" | "pdf", hasUnsavedChanges: boolean): string {
  const choice = choiceFor(format);
  const from = sourceKind === "pdf" ? "Letters reads the PDF’s text and structure, as Open PDF as Document does." : "Letters reads the document as it is saved.";
  const unsaved = hasUnsavedChanges ? " Save first to include your latest changes." : "";
  return `Makes a new .${choice.extension} file. ${from}${unsaved} This document doesn’t change.`;
}

// the plain-english descriptions of the spreadsheet screen's tools,
// joined into the app's one list in ../tools.ts. keys are written for
// windows; the explain panel says once that a mac uses command for ctrl.

import type { ToolHelp } from "../tools";

const UNTIL_SAVED = "The file changes only when you save. Undo (Ctrl Z) takes it back.";

export const sheetsTools = {
  sheetsSave: {
    label: "Save",
    tooltip: "Save your changes to this workbook",
    shortcut: "Ctrl S",
    whenToUse: "When the file on disk should have your changes. Save As (Ctrl Shift S) saves a copy as Excel (.xlsx), OpenDocument (.ods) or CSV.",
    fileEffect: "Replaces the file. Everything Letters can’t edit (charts, pictures, macros, pivot tables) is written back exactly as it came. CSV keeps only the values of one sheet, and Letters says so first.",
  },
  sheetsFormat: {
    label: "Format",
    tooltip: "Number formats, bold, colours and cell styles",
    shortcut: "Ctrl 1",
    whenToUse: "To show numbers as money, percentages or dates, or to make a header stand out. Ctrl B, Ctrl I and Ctrl U work as in Excel, and Ctrl Shift 1 to 6 give the usual number formats.",
    fileEffect: `Changes how the selected cells look, not their values. ${UNTIL_SAVED}`,
    meaning: "A number format only changes how a number is shown: 0.25 shown as a percentage is 25%, and formulas still see 0.25.",
  },
  sheetsSortFilter: {
    label: "Sort & Filter",
    tooltip: "Sort rows by the active column, or show only some of them",
    shortcut: "Ctrl Shift L",
    whenToUse: "To put a list in order, or to see only the rows with a value. Letters works on the selection, or on the block of data around the active cell.",
    fileEffect: `Sorting moves whole rows; filtering only hides rows. ${UNTIL_SAVED}`,
    meaning: "A filter hides the rows whose value you didn’t tick; the rows are still there and come back with Clear Filter.",
  },
  sheetsRowsColumns: {
    label: "Rows & Columns",
    tooltip: "Insert, delete, hide and show rows and columns",
    shortcut: "Ctrl Shift + and Ctrl −",
    whenToUse: "To make room for data or remove lines. Drag the edge of a row or column header to resize it; double-click a column’s edge to fit what is on screen.",
    fileEffect: `Formulas that point at moved cells follow them. ${UNTIL_SAVED}`,
  },
  sheetsFreeze: {
    label: "Freeze",
    tooltip: "Keep the top rows or first columns in view while scrolling",
    whenToUse: "When a long list has headings you want to see while you scroll down.",
    fileEffect: `Only changes how the sheet scrolls; Excel and LibreOffice show the same frozen panes. ${UNTIL_SAVED}`,
    meaning: "Frozen rows and columns stay on screen while the rest of the sheet scrolls under them.",
  },
  sheetsFind: {
    label: "Find",
    tooltip: "Find and replace in this sheet or the whole workbook",
    shortcut: "Ctrl F, Ctrl H to replace",
    whenToUse: "To find a value or a word in formulas. * stands for any letters and ? for one, as in Excel.",
    fileEffect: `Finding changes nothing. Replace All is one step that Undo takes back. ${UNTIL_SAVED}`,
  },
  sheetsExplainFormula: {
    label: "Explain Formula",
    tooltip: "Explain the active cell’s formula step by step, or why it shows an error",
    whenToUse: "When a formula gives an answer you don’t expect, or a cell shows #N/A, #VALUE! or another error. Each part of the formula is said in a sentence, with its value.",
    fileEffect: "Nothing, unless you choose one of the fixes offered for an error, which Undo takes back.",
  },
  sheetsAutocomplete: {
    label: "Formula Help",
    tooltip: "Suggestions while you type a formula",
    whenToUse: "Type = and a few letters, a name you half remember (VLOKUP), or what you want to do (average, count if, look up). Tab puts the function in. While a formula waits for a reference, click or drag on cells, or use the arrows, to put them in; F4 cycles A1, $A$1, A$1, $A1.",
    fileEffect: "Nothing until you press Enter to put the formula in the cell.",
    meaning: "A reference names cells: A1 is one cell, A1:A10 ten of them. The $ in $A$1 keeps it from shifting when the formula is copied.",
  },
  sheetsFillHandle: {
    label: "Fill Handle",
    tooltip: "Drag the small square at the selection’s corner to fill",
    shortcut: "Ctrl D down, Ctrl R right",
    whenToUse: "To copy a formula down a column, or continue a series such as 1, 2, 3 or Monday, Tuesday. Ctrl D and Ctrl R copy without making a series.",
    fileEffect: `Fills the cells you drag over. ${UNTIL_SAVED}`,
  },
  sheetsAutoSum: {
    label: "AutoSum",
    tooltip: "Start a SUM of the numbers above or to the left",
    shortcut: "Alt =",
    whenToUse: "Below a column of numbers, for a total. Letters puts in =SUM over the numbers above; press Enter to keep it.",
    fileEffect: `Types a formula into the active cell. ${UNTIL_SAVED}`,
  },
  sheetsTips: {
    label: "Tips",
    tooltip: "A tip when Letters sees a quicker way",
    whenToUse: "A tip appears now and then beside the grid, such as adding a total under a column or freezing a header row. Do It applies it (Undo takes it back), Not Now hides it, and Don’t Show Again stops that kind of tip.",
    fileEffect: "Nothing unless you choose Do It.",
  },
  sheetsStatus: {
    label: "Sum, Average and Count",
    tooltip: "The total, average and count of the selected cells",
    whenToUse: "Select some cells to see their sum and average at the bottom, without writing a formula.",
    fileEffect: "Nothing.",
  },
  sheetsTabs: {
    label: "Sheet Tabs",
    tooltip: "Switch, add, rename, move, duplicate or delete sheets",
    shortcut: "Ctrl Page Down and Page Up, Shift F11 to add",
    whenToUse: "Double-click a tab to rename it; right-click it for more.",
    fileEffect: `Formulas that refer to a renamed sheet follow the new name. ${UNTIL_SAVED}`,
  },
  sheetsCharts: {
    label: "Charts and Pictures",
    tooltip: "Charts, pictures and shapes are kept as they are",
    whenToUse: "A chart, picture or shape shows as a dashed card with its title where it sits. Letters can’t edit or draw charts yet; open the file in Excel or LibreOffice to change one.",
    fileEffect: "Saving keeps them exactly as they were, and a chart still shows the data it points at.",
  },
  sheetsClipboard: {
    label: "Copy and Paste",
    tooltip: "Copy, cut and paste cells, also from Excel and Google Sheets",
    shortcut: "Ctrl C, Ctrl X, Ctrl V",
    whenToUse: "Cells copied in Letters keep their formulas and formats, and the references shift as in Excel. Cells copied from Excel, Google Sheets or a web page come in as values and formulas typed into the cells.",
    fileEffect: UNTIL_SAVED,
  },
  sheetsNewSpreadsheet: {
    label: "New Spreadsheet…",
    tooltip: "Start a blank workbook or one of the starter workbooks",
    whenToUse: "For a budget, a grade sheet, lab data, a schedule or an invoice, already laid out with formulas and formats you can learn from.",
    fileEffect: "Nothing is saved until you choose Save and a place for it.",
  },
} satisfies Record<string, ToolHelp>;

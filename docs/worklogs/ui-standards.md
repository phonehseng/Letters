# ui standards worklog

`app/ui/src/lib/standards/`: the Prepare & Check sheet.

## 2026-11-12: Prepare & Check, from the document menu

PDF/A, Check This PDF, Make Smaller and Print Check live in one sheet,
opened by Prepare & Check… in the document menu (and the Forms menu on a
mac), not in a ninth inspector tab. The inspector holds tools used while
working on the pages; these act on the whole document once, give a report
and maybe a new file, and never touch the page, exactly like Export To…,
which is already a document-menu sheet. A student about to hand a thesis in
looks at the document's own menu, beside Save and Export.

PDF/A checks a chosen level (1b, 2b, 2u, 3b, each explained in a line) and
lists every finding with the engine's sentence, where it is, its clause
and whether converting fixes it. Convert and Save a Copy… writes a new
file and lists what changed, how each font was embedded (and when its
widths differ), and what still stands in the way. CHECKER_NOTE is shown as
it is under every result. Make Smaller says what each measure saved and the
engine's list of what it didn't do. Print Check reports only, and says why
Letters doesn't convert to PDF/X.

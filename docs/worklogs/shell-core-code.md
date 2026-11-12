# Worklog: code files (shell-core)

## 2026-11-12 start of the code files feature

Started `src/code`: the parts of the code editor that need no window. Plain
text files are read and written here so a byte order mark and the file's
line endings survive a save, recovery copies are kept for unsaved code, the
installed compilers are found, and syntax checks run in a throwaway folder
with a time limit. The writers that turn a word document into LaTeX,
reStructuredText and AsciiDoc live here too, because they work on the
office model and need Rust tests. Nothing in this folder ever runs the
person's program: only syntax-check flags are passed to compilers.

## 2026-11-12 text files, checks, exports and code in documents

text_file.rs gives the editor "\n" text and remembers the byte order mark,
encoding and most common line ending, so save writes the same kind of file;
invalid utf-8 is shown with replacement marks and flagged so the ui asks
before saving. The size limits (2 MB for every feature, 32 MB editable,
128 MB read-only) are measured on this computer, see the comments.

checks.rs builds the syntax-check arguments separately from running them,
so a test can assert no build or run flag is ever passed. The copy goes in
a fresh temp folder per check (removed on drop); includes resolve beside
the original through -I, read in place. runner.rs removes NODE_OPTIONS,
JAVA_TOOL_OPTIONS and the like, because they make a checker load extra
code. tools.rs skips the WindowsApps python placeholder, which opens the
Store instead of running python.

The export writers flatten the office model into a few simple blocks
(simple.rs) and render those, so LaTeX, reStructuredText, AsciiDoc, json and
csv share one walk of the document. document_code.rs finds monospaced
paragraph runs and fenced blocks in word documents, and monospaced lines in
pdfs with their indent rebuilt from x positions; hyphens are kept because
in code a trailing "-" is code. untitled.rs hands a block to a new window
as a temp file, because windows open by path.
## 2026-11-12 lists set by a paragraph style

simple.rs now treats a paragraph whose style carries a list ("List
Bullet", "List Number") as a list item. python-docx and Word's own list
styles put the list on the style, not the paragraph, and the real-app
export of such a document came out as plain paragraphs. Numbered or not
is read from the style's name, the only cheap sign without resolving the
numbering definitions.
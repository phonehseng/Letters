# Worklog: code files (ui)

## 2026-11-12 start of the code editor screen

Started `lib/code`: a small code editor screen for source and text files
(HTML, C, Python and the rest), built on CodeMirror 6 because it is MIT
licensed, accessible and loads each language on its own. The screen is
loaded only when a code file opens, so Recents and the other screens pay
nothing for it. Logic (language detection, diagnostics parsing, the
problem list, debouncing checks, finding code in documents) lives in pure
modules with tests; the components only wire them to the editor.

## 2026-11-12 editor screen, in-app checks, problems panel and compiler checks

Added the code screen (CodeScreen.svelte with CodeEditor, CodeToolbar,
CodeStatusBar, ProblemsPanel, CodeExplain) and the window routing for it:
code and text files, and any other file that turns out to be text, open
here instead of being refused. CodeScreen is imported dynamically from
App.svelte so codemirror and the language packages are a separate chunk
that Recents never loads, and each language package is imported only when
a file of that language opens (languageLoader.ts). Files over 2 MB skip
highlighting and checks, because the syntax tree of a 20 MB file costs
seconds and a lot of memory.

The checks that need no program are our own small readers (json, html,
xml, css, markdown, the bracket and quote balance) plus acorn, yaml and
smol-toml, loaded on first use. Our own json reader exists because the
browser's JSON.parse messages differ between versions and only give an
offset. The css check asks CSS.supports whether a property exists, so the
list is the browser's. Compiler output is parsed here (diagnostics.ts) and
not in Rust, so the readers can be tested with recorded samples. The
scheduler numbers each run and drops late answers; it does not cancel the
shell's previous compiler run itself, because the shell already does that
when a new check starts and a separate cancel call could arrive after the
new check and kill it.

## 2026-11-12 exports and code inside documents

ExportCodeSheet and DocumentCodeSheet are mounted by DocumentCodeTools,
which App.svelte places beside an open pdf or word document, so neither
the viewer nor the writer had to change. Export reads the document as
saved (the writer's live model isn't reachable without editing the
writer), and the sheet's line says so. Code found in documents is checked
after repairing curly quotes, non-breaking spaces, dashes and ligatures,
and each repair is listed as a warning, because code copied through a
document very often has them.
## 2026-11-12 python's lost colon, and the real-app proof

The python indentation check now also reports a line indented deeper than
the one before when that line neither opens a block nor continues
(comma, backslash, open bracket). That is how a ":" lost on a `for` line
looks, and it was the one mistake in the pdf listing proof that the basic
check missed while python itself was still starting. Proven in the debug
app: the html page's unclosed div and missing alt text, gcc 16.2's two
diagnostics with hints, the list clearing after the fix, save keeping the
byte order mark and crlf, the 20 MB file, both sheets.

## 2026-11-12 valid code the checks called wrong

A tester's valid files drew problems, and each reader now knows the
piece of syntax it was missing: python's """ strings in the indentation
check, a ";" or ":" inside css url(...), headings underlined with === or
---, shell case patterns, '...' quoting, backslashes and here-documents,
java text blocks, rust raw strings and nested comments, mysql backtick
names, and the content of an html <template> as a fragment of its own.
Each test pairs the valid example with a real mistake beside it that is
still reported.

When a compiler is installed and finishes, its problems are now the
whole list. The status line names it ("Checked by Python 3.14.7"), and
showing our basic check's guess under that name was wrong whenever the
two disagreed; the in-app checks only stand in until a compiler answers,
or when there is none. That also hides the one case left open in the
basic check, braces split across the halves of a c #ifdef, whenever gcc
or clang is installed.

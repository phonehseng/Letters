# How code is written and recorded here

Letters is meant to be read, maintained and reviewed by a person who was not
there when the code was written. These rules exist so that every line has a
reason someone can find, and so that less of it needs rewriting later.

## Every line belongs, and the reason is written down

There are three places a reason lives. Use the one that fits.

1. **In the code, as a comment that says why.** Not what the line does: the
   code already says that. Write what a reviewer would otherwise have to ask:
   why this approach, why this number, what would break without it, where a
   measured fact came from.

   ```rust
   // word squeezes spaces by up to a quarter of their width before it
   // gives up on keeping the last word (measured: ORACLE_REPORT, justified)
   const MAX_SPACE_SQUEEZE: f32 = 0.25;
   ```

2. **In the folder's `WORKLOG.md`.** One short dated entry per piece of work:
   what changed and why, in plain sentences, newest last. This is the record
   of decisions that do not belong to one line: "tables now reserve the top
   border's width per row, because Word does and long tables drifted by a line
   per page". Each crate and each feature folder of the interface has one.

3. **In the commit.** Each commit is one logical change, and its message
   says what changed and why (see "How changes land" below). Read
   `git log -p -- path/to/file` to see how a file came to be.

If you cannot say why a line is there, it should not be there. Code that is
only there "in case" gets deleted. Dead code, commented-out code and unused
parameters do not get committed.

## Every file says what it is for

A file starts with a comment of one to five lines: what it is for, and the one
idea needed to read it. A folder with more than a few files has a `README.md`
listing them, one line each.

## Readable before clever

- **Names say what a thing is.** `pages_per_search_slice`, not `n` or `pps`.
  A boolean reads as a statement: `is_scanned`, `has_text_layer`.
- **Functions do one thing and fit on a screen.** If a function needs a
  comment between its halves, it is two functions. Aim for under about forty
  lines; a long `match` with one short arm per case is fine.
- **One statement per line. No dense one-liners.** A chain of five calls gets
  named steps. A nested ternary becomes an `if`.
- **Files stay a size a person can hold in their head.** Past roughly five
  hundred lines, look for the seam and split.
- **No magic numbers.** A number that matters gets a name and a comment saying
  where it came from.
- **Errors are sentences a person can act on.** See `core/pdf/src/error.rs`.
- **The standard formatter and linter decide style.** `cargo fmt`, `cargo
  clippy`, `svelte-check`, `ruff`. Do not argue with them in code.
- **Tests read like a description of the behaviour.** The test's name is a
  sentence: `closing_a_document_frees_its_tiles`.

## Comments

Comments are lowercase and lightly punctuated, the way a student annotates
for a reviewer. They explain why, cite the measurement or the part of a
specification behind a rule, and warn about what is surprising. They do not
restate the code, apologise, or keep history ("used to be X"): history lives
in the worklog and the commits.

## How changes land

`main` must build and pass its tests at every commit, so that `git bisect`
works and anyone can check out any point in the history.

- Work happens in a local clone, on a branch named for the change
  (`fix-exact-platform-bytes`, `stage-sheets-grid`). Nothing is uploaded
  file by file.
- Each commit is one logical change that builds and passes its tests on
  its own. A change too big to review in one sitting is split into
  smaller logical steps, never into slices of a file.
- The message says what changed and why in one short line, then a few
  plain sentences if the reason needs them, for example "make the
  redaction preview report exactly what redaction will do".
- A branch reaches `main` through a pull request, once its checks pass.

## Changing someone else's code

Change the smallest thing that solves the problem. Do not reformat or reorder
what you are not changing: it buries the real change in the history. When a
rule or a number changes, change the comment and add a worklog entry saying
what was learned.

## Checking yourself

`python tools/check-readability.py` lists files and functions that have grown
too long, files with no opening comment, and folders with no worklog. It
reports; it does not fail the build. Read its list before you finish a piece
of work.

"""lists code that has become hard to read, so it can be tidied before it spreads.

it reports four things, described in docs/CONTRIBUTING.md:
  - source files with no opening comment saying what they are for
  - source files longer than a person can comfortably hold in their head
  - functions that no longer fit on a screen
  - crates and feature folders with no worklog in docs/worklogs

it never fails a build: the list is for a person to read and judge.
run from the repository root:  python tools/check-readability.py
"""

import pathlib
import re
import sys

# sizes past which a file or function deserves a second look. they are
# signals, not laws: a long table of data is fine, a long tangle is not.
MAX_FILE_LINES = 500
MAX_FUNCTION_LINES = 60

SOURCE_SUFFIXES = {".rs", ".ts", ".svelte", ".py", ".ps1", ".sh"}
SKIPPED_FOLDERS = {"target", "node_modules", "vendor", "build", ".svelte-kit", ".venv", "corpus", "fixtures", "gen"}
COMMENT_STARTS = ("//", "#", "<!--", "/*", '"""')
FUNCTION_START = re.compile(r"^\s*(?:pub(?:\([a-z]+\))?\s+)?(?:async\s+)?(?:export\s+)?(?:fn|function|def)\s+([A-Za-z_][A-Za-z0-9_]*)")


def source_files(root):
    for path in sorted(root.rglob("*")):
        if path.suffix not in SOURCE_SUFFIXES or not path.is_file():
            continue
        if SKIPPED_FOLDERS & set(path.parts) or any(part.startswith("target-") for part in path.parts):
            continue
        yield path


def has_opening_comment(lines):
    """true when the first real line of the file is a comment."""
    for line in lines[:10]:
        text = line.strip()
        if not text or text.startswith("#!") or text.startswith("<script"):
            continue
        return text.startswith(COMMENT_STARTS)
    return False


def long_functions(lines):
    """(name, length) for functions longer than the limit, measured by indentation."""
    found = []
    index = 0
    while index < len(lines):
        match = FUNCTION_START.match(lines[index])
        if not match:
            index += 1
            continue
        indent = len(lines[index]) - len(lines[index].lstrip())
        end = index + 1
        # the function ends at the next line indented no deeper than its start
        # that is not blank and is not just a closing bracket
        while end < len(lines):
            text = lines[end]
            if text.strip() and (len(text) - len(text.lstrip())) <= indent and text.strip() not in ("}", "})", "};", ")"):
                break
            end += 1
        length = end - index
        if length > MAX_FUNCTION_LINES:
            found.append((match.group(1), length))
        index = end
    return found


def worklog_name(root, folder):
    """the name a folder's worklog has in docs/worklogs (see CONTRIBUTING.md)."""
    parts = folder.relative_to(root).parts
    if parts[0] == "core":
        return f"core-{parts[1]}"
    if parts[:4] == ("app", "ui", "src", "lib"):
        return f"ui-{parts[4]}"
    return parts[1]


# folders that keep one worklog per feature inside them instead of one of
# their own: shell-core's features (shell-core-ocr.md) and the interface's
# (ui-writer.md)
PER_FEATURE = ("shell-core", "ui")


def has_worklog(root, folder):
    worklogs = root / "docs" / "worklogs"
    name = worklog_name(root, folder)
    if name in PER_FEATURE:
        return any(worklogs.glob(f"{name}-*.md"))
    return (worklogs / f"{name}.md").is_file()


def folders_needing_worklogs(root):
    """every crate, and every feature folder of the interface, should keep a worklog."""
    folders = [path for path in (root / "core").iterdir() if path.is_dir()]
    folders += [path for path in (root / "app").iterdir() if path.is_dir()]
    library = root / "app" / "ui" / "src" / "lib"
    if library.is_dir():
        folders += [path for path in library.iterdir() if path.is_dir()]
    return [folder for folder in folders if not has_worklog(root, folder)]


def main():
    root = pathlib.Path(__file__).resolve().parent.parent
    no_comment, long_files, long_fns = [], [], []
    for path in source_files(root):
        try:
            lines = path.read_text(encoding="utf-8").splitlines()
        except (UnicodeDecodeError, OSError):
            continue
        name = path.relative_to(root).as_posix()
        if lines and not has_opening_comment(lines):
            no_comment.append(name)
        if len(lines) > MAX_FILE_LINES:
            long_files.append((len(lines), name))
        for function, length in long_functions(lines):
            long_fns.append((length, name, function))

    def section(title, rows):
        print(f"\n{title}: {len(rows)}")
        for row in rows[:60]:
            print("  " + row)
        if len(rows) > 60:
            print(f"  ... and {len(rows) - 60} more")

    section("files with no opening comment", no_comment)
    section(f"files over {MAX_FILE_LINES} lines", [f"{count:5}  {name}" for count, name in sorted(long_files, reverse=True)])
    section(
        f"functions over {MAX_FUNCTION_LINES} lines",
        [f"{length:4}  {name}  {function}" for length, name, function in sorted(long_fns, reverse=True)],
    )
    section("folders with no worklog in docs/worklogs", [folder.relative_to(root).as_posix() for folder in folders_needing_worklogs(root)])
    return 0


if __name__ == "__main__":
    sys.exit(main())

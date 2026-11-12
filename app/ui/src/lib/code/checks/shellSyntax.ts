// the parts of shell syntax the bracket check has to know so valid
// scripts stay quiet: a backslash outside quotes, here-documents, and the
// ")" that ends a case pattern, which has no "(" of its own.

/** a backslash outside quotes makes the next character plain: echo \( or it'\''s */
export function skipShellEscape(text: string, index: number): number {
  return text[index] === "\\" ? Math.min(index + 2, text.length) : index;
}

// <<EOF, <<-EOF, <<'EOF' and <<"EOF"; the lookbehind keeps <<< (a here-string) out
const HEREDOC = /(?<!<)<<(-?)\s*(['"]?)([A-Za-z_][A-Za-z0-9_]*)\2/g;

/**
 * at the newline that ends a line starting here-documents, the offset after
 * their bodies (the lines up to each closing word), or `index` itself. a
 * here-document is text, so its brackets and quotes never count. when the
 * closing word never comes, the "<<" was something else (a shift inside
 * $(( ))), and nothing is skipped.
 */
export function skipHeredocBodies(text: string, index: number): number {
  if (text[index] !== "\n") return index;
  const lineStart = text.lastIndexOf("\n", index - 1) + 1;
  let bodyStart = index + 1;
  for (const heredoc of text.slice(lineStart, index).matchAll(HEREDOC)) {
    // <<- lets the closing word be indented with tabs
    const closing = heredoc[1] === "-" ? new RegExp(`^\\t*${heredoc[3]}\\r?$`, "m") : new RegExp(`^${heredoc[3]}\\r?$`, "m");
    const found = closing.exec(text.slice(bodyStart));
    if (!found) return index;
    bodyStart += found.index + found[0].length;
  }
  // the newline after the last closing word is scanned as usual
  return bodyStart === index + 1 ? index : bodyStart;
}

/** a case pattern such as start, *, "a"|b or [yY]*, optionally after its own "(". */
const CASE_PATTERN = /^\s*\(?\s*[^\s()|;&<>]+(\s*\|\s*[^\s()|;&<>]+)*\s*$/;

/**
 * true when the ")" at `index` ends a case pattern ("start) echo starting ;;").
 * the text before it, back to the line start, a ";;" or "case … in", must be
 * just a pattern, and a case must be open: an extra ")" elsewhere is still
 * reported.
 */
export function endsCasePattern(text: string, index: number): boolean {
  const before = text.slice(0, index);
  const lineStart = before.lastIndexOf("\n") + 1;
  // ";;" ends the case item before; ";&" and ";;&" are bash's fall-through forms
  const separator = Math.max(before.lastIndexOf(";;"), before.lastIndexOf(";&"));
  const patternStart = separator >= lineStart ? separator + 2 : lineStart;
  // the "&" left over from ";;&", and "case $x in" when the pattern shares its line
  const pattern = before.slice(patternStart).replace(/^&/, "").replace(/^\s*case\s+.*?\s+in\s/, "");
  return CASE_PATTERN.test(pattern) && openCaseCount(before) > 0;
}

/** how many "case" words have no "esac" yet; comments and strings may miscount, which is rare. */
function openCaseCount(before: string): number {
  const opened = before.match(/(^|[\s;(])case\s/g)?.length ?? 0;
  const closed = before.match(/(^|[\s;])esac(?![\w-])/g)?.length ?? 0;
  return opened - closed;
}

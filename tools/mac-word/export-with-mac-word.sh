#!/bin/bash
# exports every docx in this kit to pdf with microsoft word for mac, and
# packs the pdfs with a note of the word and macos versions into one zip
# to send back. it only reads the kit and writes inside this folder and
# word's own container folder; it never opens or closes anything else.
set -euo pipefail

here="$(cd "$(dirname "$0")" && pwd)"
word_app="/Applications/Microsoft Word.app"
# word for mac is sandboxed: it can read and write inside its own
# container folder without asking for each file, so the work happens there
container="$HOME/Library/Containers/com.microsoft.Word/Data"
work="$container/LettersMacWordCheck"
results="$here/results"

if [ ! -d "$word_app" ]; then
    echo "Microsoft Word isn't in /Applications. Install it (or move it there), then run this again."
    exit 1
fi
if pgrep -xq "Microsoft Word"; then
    echo "Please quit Microsoft Word first (Word > Quit Word), then run this again."
    echo "Starting from a closed Word keeps your own documents out of the way."
    exit 1
fi
if [ ! -d "$container" ]; then
    echo "Word's folder doesn't exist yet. Open Word once, close the start screen, quit it, then run this again."
    exit 1
fi

rm -rf "$work" "$results"
mkdir -p "$work/docx" "$work/pdf" "$results/pdf"
cp "$here"/docx/*.docx "$work/docx/"

{
    echo "word_version=$(defaults read "$word_app/Contents/Info" CFBundleShortVersionString)"
    echo "word_build=$(defaults read "$word_app/Contents/Info" CFBundleVersion)"
    sw_vers
    echo "machine=$(uname -m)"
    echo "started=$(date -u +%Y-%m-%dT%H:%M:%SZ)"
} > "$results/about.txt"

total=$(ls "$work/docx" | wc -l | tr -d ' ')
count=0
failed=0
for docx in "$work"/docx/*.docx; do
    count=$((count + 1))
    name="$(basename "$docx" .docx)"
    echo "[$count/$total] $name"
    # one file at a time, so one failure doesn't stop the rest
    if ! osascript "$here/export-one.applescript" "$docx" "$work/pdf/$name.pdf" 2>>"$results/errors.txt"; then
        echo "    failed (the reason is in results/errors.txt)"
        failed=$((failed + 1))
    fi
done

osascript -e 'tell application "Microsoft Word" to quit saving no' || true
cp "$work"/pdf/*.pdf "$results/pdf/" 2>/dev/null || true
echo "finished=$(date -u +%Y-%m-%dT%H:%M:%SZ)" >> "$results/about.txt"
rm -rf "$work"

zip_file="$here/letters-mac-word-results.zip"
rm -f "$zip_file"
ditto -c -k --keepParent "$results" "$zip_file"
echo
echo "Done: $((count - failed)) of $total exported."
echo "Send this file back: $zip_file"

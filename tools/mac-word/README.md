# Checking the exact converter against Word for Mac

Every fidelity number so far was measured through Word 16.0.20527 on
Windows. This checks whether Word for Mac places the converter's text in
the same places. The 13 corpus documents go through Mac Word's own PDF
export, and the result is measured with the converter's own harness and
targets: every glyph within 0.125 pt of the source, mean under 0.05 pt.

## What you need on the Mac

- Microsoft Word for Mac, signed in and activated, in `/Applications`.
- Terminal (in Applications > Utilities).
- About 15 minutes. Nothing else to install: the script uses only what
  macOS already has.

## Steps

1. **Quit Word** if it's open (Word > Quit Word). The script refuses to run
   while Word is open, so your own documents are never touched.
2. If you have never opened Word on this Mac, open it once, close the start
   screen and quit it. That creates the folder Word works in.
3. **Download the kit**, `letters-mac-word-kit.zip`, onto the Mac. It holds
   39 `.docx` files (13 documents, three ways each), the two scripts in this
   folder and a copy of this page.
4. In Terminal, run these one at a time:

   ```bash
   cd ~/Downloads
   ```

   ```bash
   unzip -o letters-mac-word-kit.zip
   ```

   ```bash
   bash letters-mac-word-kit/export-with-mac-word.sh
   ```

5. The first time, macOS asks whether Terminal may control Microsoft Word.
   Choose **Allow**. Word then opens each file, saves it as a PDF and closes
   it, one by one; you'll see its windows come and go. Don't type in Word
   while it runs.
6. When it says `Done`, **send back one file**:
   `~/Downloads/letters-mac-word-kit/letters-mac-word-results.zip`.
   It holds the 39 PDFs, `about.txt` (Word's version and build, macOS
   version, Intel or Apple chip) and `errors.txt` if anything failed.

## If the script fails

The script has not yet been run on a Mac, so Word's scripting may need a
change for your version. Send back `results/errors.txt` along with what
Terminal printed.

If you'd like results anyway, export a few by hand: open
`letters-mac-word-kit/docx/syllabus.locked.docx` in Word, then
File > Save As, set File Format to **PDF**, and choose **Best for printing**.
Don't choose the option that uses Microsoft's online service: it uploads
the file. Save it as `syllabus.locked.pdf` and send it back. Do the same for
`syllabus.fitted.docx` and `journal.locked.docx` if you have time.

## What happens with the results (on Windows)

```bash
cargo run -p letters-exact --example mac_word -- measure <kit folder> <results/pdf>
```

For each document this prints:

- **Locked layout**, mac vs source and windows vs source: max and mean glyph
  error, and pass or fail against the 0.125 / 0.05 pt targets. This is the
  direct answer for the line-by-line mode.
- **Fitted fonts**, mac vs windows origins: how far apart the two Words
  print each word's glyph. Both read the same embedded fonts from the same
  file, so equal origins mean Mac Word is as exact as the 0.0066 pt measured
  on Windows. Any difference adds directly to that error.
- The PDF's producer shows which export Mac Word used.

The kit itself is made on Windows with Word for Windows:

```bash
cargo run -p letters-exact --example mac_word -- kit <kit folder>
```

## If Mac Word lays out text differently

Word for Windows snaps text to a 1/600 inch grid, and the converter's step
tables and fitted fonts are calibrated to that. If Mac Word differs, the
plan is:

1. **Measure Mac Word as Windows Word was measured.** Run the same probe
   pages (`measured_step_tables_agree_with_the_built_in_ones` in
   `tests/word.rs`) through Mac Word, and record a second set of step tables
   for the Mac build. The model already keeps facts per Word build
   (`WordModel.build`), so this becomes a second built-in calibration rather
   than a change to the first.
2. **Choose by the Word that will open the file, not the one converting
   it.** A student converting on Windows may hand the file to someone on a
   Mac. The Export dialog gains "Made for: Word for Windows / Word for Mac",
   set at first to the Word installed on this computer. It's found the same
   way as now on Windows (the registry); on a Mac, from
   `/Applications/Microsoft Word.app/Contents/Info.plist`.
3. **Check through the right Word.** On a Mac, the checking export and the
   fitted fonts' measuring export run through Mac Word with AppleScript
   (`export-one.applescript` is the start of that bridge). Without the
   target Word on the computer, the converter uses the built-in tables for
   that target and says the result wasn't checked, as it does today.
4. **Say which Word a file was made for.** The target is written into the
   docx's custom properties, so a later check can tell what the file was
   calibrated against.

# The writer's interface (app/ui/src/lib/writer)

Word-processor mode: the editing surface, its objects, and the calls into
the shell. How the pieces fit is in ARCHITECTURE.md (Word-processor mode);
why each piece changed is in WORKLOG.md. `inspector/` holds the Format,
Document and Review tabs; `fixtures/` the models the round-trip tests read.

| File | What it is for |
|---|---|
| `clipboardBridge.ts` | the system clipboard and the writer, through the office engine |
| `CompatibilitySheet.svelte` | Strict Word mode's "Show" list |
| `convert.test.ts` | model -> editor -> model must give back exactly what the engine read, for every kind of content |
| `convert.ts` | the office model (json from the shell) to an editor document and back |
| `counts.ts` | the status bar's numbers |
| `crop.ts` | cropping and rotating pictures |
| `css.ts` | how model formatting looks on screen |
| `DriveSheet.svelte` | Save to Google Drive |
| `editing.ts` | the editing commands every control of the writer uses |
| `explainList.ts` | what the writer's Explain panel lists |
| `fileActions.ts` | the writer's ways out of the editor |
| `find.ts` | find and replace in the editor document |
| `FindReplace.svelte` | find and replace |
| `fonts.test.ts` | the fonts a document uses and replacing one everywhere (fonts.ts) |
| `fonts.ts` | fonts, behind one small adapter |
| `formats.ts` | the formats Save As offers, from the office engine's table (formats::format_capabilities, through writerApi.saveFormats), each with the engine's sentence |
| `formatting.test.ts` | styles, marks, paragraph formatting, lists, indents and inserting |
| `frameMaths.ts` | the object frame's maths, kept out of the component so it can be tested |
| `images.ts` | pictures in the editor |
| `indentation.ts` | "smart indentation" |
| `layoutCheck.svelte.ts` | asking Word (Strict Word mode) or Letters' own engine (a setting, Free mode) for the document's lines and pages, without ever making typing wait |
| `layoutRules.test.ts` | pages in Free mode, Word's lines and pages applied to the editor, and the clipboard helpers |
| `layoutSource.ts` | where the page breaks on the editing surface come from |
| `MiniEditor.svelte` | a small editor for a header, a footer or a footnote |
| `model.ts` | the office document model as the shell sends it (core/office/src/model) |
| `numbering.ts` | lists |
| `ObjectBar.svelte` | the small glass bar above a selected picture or text block |
| `ObjectFrame.svelte` | the frame around the selected picture or text block |
| `objectGeometry.ts` | where free objects (free pictures and text blocks) sit on the editing surface, worked out from the model exactly as word places them |
| `objectLayer.ts` | the editor's object layer |
| `objects.test.ts` | where free objects sit, snapping, text wrap, cropping and turning |
| `outline.ts` | the outline panel |
| `pageContent.css` | the document's own content on the page surface (PageSurface.svelte) |
| `pageGaps.ts` | the space between two pages on the editing surface |
| `pageMeasure.ts` | measuring the editing surface for pagination (PageSurface.svelte) |
| `PageSurface.svelte` | the document as pages on the grey desk, like the pdf viewer |
| `pagination.ts` | where page boundaries go on the editing surface (Free mode, and Strict mode whenever word's layout isn't there yet) |
| `pictureCommands.ts` | pictures from the outside and the editor's own in line or free toggle |
| `plugins.ts` | the editor's behaviour |
| `preserved.ts` | content the model keeps exactly as the file had it (a chart, a text box, an equation...) |
| `print.ts` | printing through the system's print dialog |
| `PromptSheet.svelte` | a small sheet under the toolbar that asks for one thing, such as a link's address or a comment's words |
| `PullDown.svelte` | a toolbar pull-down menu (Document, Style, Insert, View) |
| `resize.test.ts` | resizing pictures, the Save As formats, where page breaks come from, strict mode and printing |
| `resize.ts` | resizing and moving pictures |
| `routing.ts` | which screen a file opens in |
| `Ruler.svelte` | the ruler above the page |
| `ruler.test.ts` | the ruler |
| `ruler.ts` | the ruler above the page |
| `schema.ts` | the editor's schema |
| `shortcuts.ts` | the writer's keyboard shortcuts |
| `snapping.ts` | snap guides while an object is dragged |
| `StatusBar.svelte` | the strip along the bottom of the writer |
| `strictMode.ts` | Strict Word mode |
| `structure.test.ts` | outline, find and replace, word counts and the page estimate |
| `styles.ts` | styles |
| `surfaceLayout.ts` | reading the editing surface back after it is drawn |
| `tableEdits.ts` | changes to the table around the cursor, as transactions the session runs |
| `TableGrid.svelte` | the table picker at the top of the Insert menu |
| `testDocs.ts` | small documents for the writer's tests |
| `textBlocks.test.ts` | text blocks |
| `textBlocks.ts` | text blocks (the model's `textBox`) and the other commands the office engine runs on the page's objects (shell-core/src/writer/objects.rs) |
| `tracked.test.ts` | tracked changes |
| `tracked.ts` | tracked changes |
| `trackedFormatting.ts` | tracked formatting changes and tracked paragraph splits and joins, in the forms word itself stores them, so they save as real revisions |
| `typing.test.ts` | the same cases as the tests in core/office/src/typing.rs, so the port keeps its behaviour |
| `typing.ts` | what happens while someone types |
| `TypingToggles.svelte` | the four typing switches |
| `units.ts` | lengths |
| `wordLines.ts` | Strict Word mode |
| `wrap.ts` | text flowing around free objects on the editing surface |
| `writerApi.ts` | every call from the writer into the desktop shell (see app/shell/src/writer_commands.rs), so the writer's components never need to know command names |
| `WriterButton.svelte` | a toolbar or inspector button of the writer |
| `writerCommands.ts` | the writer's commands, by name |
| `WriterExplain.svelte` | Explain for the writer |
| `writerGuard.ts` | how the window asks the writer before it closes or goes back to Recents |
| `WriterIcon.svelte` | the writer's symbols, drawn like the app's own (Icon.svelte) |
| `WriterInspector.svelte` | the writer's inspector on the trailing side, like Pages' Format and Document inspectors |
| `WriterNotices.svelte` | the writer's notice bar under the toolbar (the recovered version, the session's messages with their one action, the question before saving as .odt) and the small sheets that ask for a link or a comment |
| `writerObjects.svelte.ts` | the commands on the page's objects |
| `WriterOutline.svelte` | the outline in the sidebar |
| `WriterScreen.svelte` | the writer |
| `writerSession.svelte.ts` | one open writer document |
| `WriterToolbar.svelte` | the writer's floating toolbar, in the viewer's three areas |
| `writerTools.ts` | the plain-english description of every writer tool, in the same form as ../tools.ts (which includes this list, so Explain and tooltips find them |

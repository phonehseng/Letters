# The Compare screen

Two versions of a PDF side by side or laid over each other, with the changes
listed. The engine is core/compare; the shell's part is
app/shell-core/src/compare and app/shell/src/compare_commands.rs.

| File | Job |
|---|---|
| `CompareScreen.svelte` | the screen: choosing, progress, results, sheets, keys |
| `CompareSetupView.svelte`, `CompareOptionsForm.svelte` | the older and newer version, Swap, the options |
| `CompareProgress.svelte` | the bar, the stage in words, Stop |
| `CompareToolbar.svelte` | Back, the view, zoom, Link Scrolling, Options, Save Report, Changes, Explain |
| `CompareColumn.svelte`, `ChangeMarks.svelte` | one side of Side by Side, and the tinted marks on a page |
| `OverlayView.svelte` | the overlay pictures, the Old to New slider, Flash |
| `ChangesList.svelte` | the sidebar: summary, filters, Previous and Next, the virtual list |
| `OptionsSheet.svelte`, `SaveReportSheet.svelte` | Compare Again with other options; the report's format |
| `compareApi.ts` | the shell's commands and the engine's result, typed |
| `changes.ts` | grouping, filters, stepping, sentences, mark tones |
| `scrollSync.ts` | slots per pair, the scroll map, following scroll |
| `changeList.ts` | the virtual list's rows |
| `overlay.ts`, `progress.ts`, `setup.ts`, `report.ts` | the slider and Flash; progress; choosing versions; the report's words |
| `compareCommands.ts`, `compareKeys.ts`, `compareTools.ts` | command names, the screen's own dispatcher, the words of each control |
| `compare.test.ts` | tests of the pure modules |
| `WORKLOG.md` | what changed and why |

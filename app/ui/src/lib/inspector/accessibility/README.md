# The Accessibility tab's sections

| File | Job |
|---|---|
| `CheckSection.svelte`, `FindingItem.svelte` | Check This Document: the score and the grouped findings |
| `FixSection.svelte`, `FixStep.svelte`, `Decisions.svelte` | Fix Problems…: the wizard, the form for each step, what auto-tagging guessed |
| `PdfUaSection.svelte` | Mark as PDF/UA, with the checks by hand ticked by the person |
| `ReportSection.svelte` | Save Report… as a web page or plain text |
| `fixes/*Fix.svelte` | one small form per kind of fix; `RegionPicture.svelte` shows a part of a page |

The logic and its worklog are in `lib/a11y`.

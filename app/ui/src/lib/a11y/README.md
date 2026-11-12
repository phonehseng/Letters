# The Accessibility tab's logic

The checker and fix wizard's shared state and pure modules. The tab itself is
`inspector/AccessibilityTab.svelte` and `inspector/accessibility/`; the
engine is core/a11y; the shell's part is app/shell-core/src/a11y and
app/shell/src/a11y_commands.rs.

| File | Job |
|---|---|
| `a11yApi.ts` | the shell's commands, the report and the fix requests, typed |
| `a11ySession.svelte.ts` | the report, the wizard, the outlined spot, the numbered reading order |
| `A11yMarks.svelte` | what is outlined on a page |
| `findings.ts` | Must Fix / Should Fix / Check by Hand, where, a key across checks |
| `wizard.ts` | the wizard's steps, Skip, Back, refreshing after a check |
| `ladder.ts` | the heading ladder and moving blocks in the reading order |
| `languages.ts` | languages by name with their codes |
| `contrast.ts`, `tokenAudit.ts` | WCAG contrast maths, and the audit of app.css's tokens |
| `a11yCommands.ts`, `a11yTools.ts` | command names and the words of each control |
| `a11y.test.ts` | tests of the pure modules |
| `WORKLOG.md` | what changed and why |

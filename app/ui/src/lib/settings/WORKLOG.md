# Worklog: app/ui/src/lib/settings

## 2026-11-11 the Settings window

Added `settingsModel.ts` (reading any version of the stored file, provider
switching, what is missing, the privacy summary; tested), `SettingsScreen`,
`AccountsSection`, `AiSection` and `settingsApi.ts`. Changes are saved at
once, as in a mac's Settings. Typing and Google Drive reuse the writer's own
settings and drive module so both places agree.

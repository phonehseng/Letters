# Worklog: app/ui/src/lib/ai

## 2026-11-11 the AI Tools panel

Added `aiPanel.ts` (a request's steps as a state machine: plan, confirm when
text would leave, run, answer; the header line; answers cut into text and
citation chips; tested), `AiPanel.svelte`, `DisclosureSheet.svelte` and
`aiApi.ts`. Cancel has the focus in the confirmation because Send is the
choice that sends something away. With the tools off only Find a Topic shows.

## 2026-11-12 the AI Tools button starts on the first tab

Opening the panel from its button resets the tab, because after Find a Topic
the panel reopened on that tab and Summarize was not reachable (seen in the
real-app proof).

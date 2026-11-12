// the Compare screen's own command dispatcher. the screen shows over a
// document that stays open (and keeps its commands) underneath, so while
// it shows the window sends commands here instead of to the app's
// dispatcher (see App.svelte). when it closes, the document's commands
// are still there, untouched.

import { createDispatcher } from "../commands";

export const compareCommands = createDispatcher();

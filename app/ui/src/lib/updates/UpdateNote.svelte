<script lang="ts">
  // the note that a newer version of Letters is ready: what changed, in a
  // few plain lines, and Update and Restart or Later. updating closes every
  // window the usual way first, so unsaved changes are asked about.
  import { asCommandError } from "../api";
  import { prepareUpdate, updateHeading, type UpdateNote } from "./updatesApi";

  interface Props {
    note: UpdateNote;
    onLater: () => void;
  }

  let { note, onLater }: Props = $props();

  let busy = $state(false);
  let problem = $state<string | null>(null);

  async function update() {
    busy = true;
    problem = null;
    try {
      // the windows are asked to close once the update is downloaded and checked
      await prepareUpdate();
    } catch (error) {
      problem = asCommandError(error).message;
      busy = false;
    }
  }
</script>

<div class="glass note" role="dialog" aria-label={updateHeading(note)}>
  <p class="title">{updateHeading(note)}</p>
  <p class="detail">You have {note.current}. {note.notes || "This version fixes problems and makes Letters better."}</p>
  {#if problem}
    <p class="problem" role="alert">{problem}</p>
  {/if}
  <p class="detail">Your settings, recent files and saved versions stay as they are. Letters asks about any unsaved changes before it restarts.</p>
  <div class="buttons">
    <button class="button" onclick={onLater} disabled={busy}>Later</button>
    <button class="button primary" onclick={update} disabled={busy}>{busy ? "Downloading…" : "Update and Restart"}</button>
  </div>
</div>

<style>
  .note {
    position: fixed;
    right: 1rem;
    bottom: 3rem;
    z-index: 30;
    width: min(24rem, calc(100% - 2rem));
    padding: 1rem 1.125rem;
    border-radius: 0.875rem;
  }

  .title {
    margin: 0 0 0.375rem;
    font: var(--headline);
  }

  .detail {
    margin: 0 0 0.5rem;
    color: var(--label-secondary);
  }

  .problem {
    margin: 0 0 0.5rem;
    color: var(--danger);
  }

  .buttons {
    display: flex;
    justify-content: flex-end;
    gap: 0.5rem;
  }
</style>

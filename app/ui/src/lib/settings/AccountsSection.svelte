<script lang="ts">
  // Accounts: the Google Drive client id the writer's Drive sheet asks
  // for, and the signed-in account with Sign Out. it uses the writer's
  // drive module and settings, so both places always agree.
  import { onMount } from "svelte";
  import { driveSignOut, driveStatus, openLink, saveWriterSettings, writerSettings, type DriveStatus, type WriterSettings } from "../writer/writerApi";

  let status = $state<DriveStatus | null>(null);
  let settings = $state<WriterSettings | null>(null);
  let clientId = $state("");

  onMount(refresh);

  async function refresh() {
    settings = await writerSettings();
    clientId = settings.driveClientId ?? "";
    status = await driveStatus().catch(() => ({ folders: [], canUpload: false, signedIn: false }));
  }

  async function saveClientId() {
    if (!settings) return;
    settings = await saveWriterSettings({ ...settings, driveClientId: clientId.trim() });
    await refresh();
  }

  async function signOut() {
    await driveSignOut();
    await refresh();
  }
</script>

<section>
  <h2>Google Drive</h2>
  <label class="setting">
    <span class="caption">Client ID</span>
    <input class="field" bind:value={clientId} placeholder="….apps.googleusercontent.com" onchange={saveClientId} />
  </label>
  <p class="hint">
    Uploading to Drive directly needs a Google OAuth client ID for desktop apps, which whoever publishes this copy of Letters registers with Google. Letters doesn’t ship one. Saving into the Google Drive folder on this computer needs none.
  </p>
  <button class="link" onclick={() => openLink("https://developers.google.com/identity/protocols/oauth2/native-app")}>How to Get a Client ID</button>
  {#if status}
    <p>
      {#if status.signedIn}Signed in to Google Drive. Letters can see only the files it uploaded.{:else}Not signed in.{/if}
    </p>
    {#if status.signedIn}
      <button class="button" onclick={signOut}>Sign Out</button>
    {/if}
  {/if}
</section>

<style>
  section {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 0.5rem;
  }

  h2 {
    margin: 0;
    font: var(--headline);
  }

  p {
    margin: 0;
  }

  .setting .field {
    width: min(22rem, 100%);
  }

  .link {
    padding: 0;
    border: 0;
    background: none;
    color: var(--accent-label);
  }
</style>

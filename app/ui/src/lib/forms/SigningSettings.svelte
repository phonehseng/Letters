<script lang="ts">
  // Settings > Signing: the address of the time stamp server a signature
  // may ask for a trusted time. empty means none, and then nothing is ever
  // sent: signatures use this computer's clock and say so. the engine's
  // disclosure is shown here too, so the choice is made knowing what goes.
  import { onMount } from "svelte";
  import type { AppSettings } from "../settings/settingsModel";
  import { signingNotes, type SigningNotes } from "./signingApi";

  interface Props {
    settings: AppSettings;
    onChange: (update: (draft: AppSettings) => void) => void;
  }

  let { settings, onChange }: Props = $props();

  let notes = $state<SigningNotes | null>(null);
  let address = $state("");
  const problem = $derived(addressProblem(address));

  onMount(() => {
    address = settings.signing.timestampServer;
    signingNotes()
      .then((read) => (notes = read))
      .catch(() => (notes = null));
  });

  function addressProblem(typed: string): string | null {
    const trimmed = typed.trim();
    if (!trimmed) return null;
    try {
      const url = new URL(trimmed);
      return url.protocol === "https:" || url.protocol === "http:" ? null : "Type an address that starts with https://.";
    } catch {
      return "Type the server’s whole address, such as https://tsa.example.org/tsr.";
    }
  }

  function save() {
    if (problem) return;
    const value = address.trim();
    onChange((draft) => (draft.signing = { timestampServer: value }));
  }
</script>

<label class="setting">
  <span class="caption">Time Stamp Server</span>
  <input class="field" placeholder="https://tsa.example.org/tsr" bind:value={address} onchange={save} />
  {#if problem}
    <span class="hint" role="alert">{problem}</span>
  {:else}
    <span class="hint">Leave it empty to sign with this computer’s clock. Your school or employer can tell you which server to use.</span>
  {/if}
</label>
{#if notes}
  <p class="hint">{notes.timestampDisclosure}</p>
{/if}

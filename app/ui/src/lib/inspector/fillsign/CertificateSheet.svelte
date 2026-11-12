<script lang="ts">
  // Create a Self-Signed Certificate…: a name, an email, and a password
  // that protects the new file. it explains what self-signed means before
  // anything is made: you vouch for yourself, so others' apps show the
  // signature as not verified. the file is saved where the person says,
  // made entirely on this computer.
  import { onMount } from "svelte";
  import { asCommandError } from "../../api";
  import { chooseCertificateLocation, createCertificate, type CertificateMade } from "../../forms/formsApi";
  import { tools } from "../../tools";

  interface Props {
    /** what was made (and the password typed, to sign with it now), or null when cancelled */
    onDone: (made: CertificateMade | null, password: string) => void;
    onError: (message: string) => void;
  }

  let { onDone, onError }: Props = $props();

  let name = $state("");
  let email = $state("");
  let password = $state("");
  let again = $state("");
  let busy = $state(false);
  let made = $state<CertificateMade | null>(null);
  let sheet = $state<HTMLFormElement>();
  let first = $state<HTMLInputElement>();

  const problem = $derived(
    !name.trim()
      ? "Type the name to sign with."
      : password.length < 6
        ? "Choose a password of at least 6 characters."
        : password !== again
          ? "The two passwords aren’t the same."
          : null,
  );

  onMount(() => first?.focus());

  async function create(event: SubmitEvent) {
    event.preventDefault();
    if (problem || busy) return;
    const path = await chooseCertificateLocation(`${name.trim()}.p12`);
    if (!path) return;
    busy = true;
    try {
      made = await createCertificate({ name: name.trim(), email: email.trim() || null, validDays: 3 * 365, password, keyKind: "rsa2048" }, path);
    } catch (problem) {
      onError(asCommandError(problem).message);
    } finally {
      busy = false;
    }
  }

  function onKeyDown(event: KeyboardEvent) {
    if (event.key === "Escape") {
      event.preventDefault();
      event.stopPropagation();
      onDone(made, password);
    } else if (event.key === "Tab") {
      // the sheet is modal, so Tab goes round its own controls
      const controls = [...(sheet?.querySelectorAll<HTMLElement>("input, button") ?? [])].filter((each) => !each.hasAttribute("disabled"));
      const index = controls.indexOf(window.document.activeElement as HTMLElement);
      const next = controls[(index + (event.shiftKey ? -1 : 1) + controls.length) % controls.length];
      event.preventDefault();
      next?.focus();
    }
  }
</script>

<div class="backdrop">
  <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
  <div role="dialog" tabindex="-1" aria-modal="true" aria-labelledby="certificate-title" class="dialog" onkeydown={onKeyDown}>
  <form class="glass sheet" bind:this={sheet} onsubmit={create}>
    <h2 id="certificate-title">Create a Self-Signed Certificate</h2>
    {#if made}
      <p>Saved “{made.path.split(/[\\/]/).pop()}”. It can be used until {made.validUntil}.</p>
      <p class="note">{made.note}</p>
      <div class="buttons">
        <span class="space"></span>
        <button class="button primary" type="button" onclick={() => onDone(made, password)}>Use This Certificate</button>
      </div>
    {:else}
      <p>{tools.selfSignedCertificate.meaning}</p>
      <label class="setting">
        <span class="caption">Name</span>
        <input class="field" bind:this={first} bind:value={name} autocomplete="name" />
      </label>
      <label class="setting">
        <span class="caption">Email (optional)</span>
        <input class="field" type="email" bind:value={email} autocomplete="email" />
      </label>
      <label class="setting">
        <span class="caption">Password for the File</span>
        <input class="field" type="password" bind:value={password} autocomplete="new-password" />
      </label>
      <label class="setting">
        <span class="caption">Password Again</span>
        <input class="field" type="password" bind:value={again} autocomplete="new-password" />
      </label>
      <p class="hint" aria-live="polite">{problem ?? "Keep the file and its password safe: anyone who has both can sign as you."}</p>
      <div class="buttons">
        <span class="space"></span>
        <button class="button" type="button" onclick={() => onDone(null, "")}>Cancel</button>
        <button class="button primary" type="submit" disabled={problem !== null || busy}>Create…</button>
      </div>
    {/if}
  </form>
  </div>
</div>

<style>
  .backdrop {
    position: fixed;
    inset: 0;
    z-index: 10;
    display: flex;
    justify-content: center;
    align-items: flex-start;
    background: rgb(0 0 0 / 0.12);
  }

  .dialog {
    display: contents;
  }

  .sheet {
    display: flex;
    flex-direction: column;
    gap: 0.625rem;
    width: min(26rem, calc(100% - 2rem));
    max-height: calc(100% - var(--toolbar-space) - 1rem);
    margin-top: var(--toolbar-space);
    padding: 1.25rem 1.25rem 1rem;
    overflow-y: auto;
    background: var(--window);
    animation: drop 0.2s var(--ease);
  }

  h2 {
    margin: 0;
    font: var(--headline);
  }

  p {
    margin: 0;
    color: var(--label-secondary);
    line-height: 1.125rem;
  }

  .note {
    color: var(--label);
  }

  .buttons {
    display: flex;
    gap: 0.5rem;
    margin-top: 0.5rem;
  }

  .space {
    flex: 1;
  }

  @keyframes drop {
    from {
      opacity: 0;
      transform: translateY(-0.75rem);
    }
  }
</style>

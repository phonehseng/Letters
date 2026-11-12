<script lang="ts">
  // the extras of Sign with a Certificate, under a disclosure so the
  // common signature stays one click: a trusted time from the server
  // chosen in Settings (with the engine's disclosure shown before anything
  // is sent), certifying in plain words, locking fields, following the
  // signing order, storing validation data, and the audit trail.
  import type { FieldInfo } from "../../forms/formsApi";
  import type { CertificationLevel, SigningChoices, SigningNotes } from "../../forms/signingApi";
  import { CERTIFY_HINTS, CERTIFY_LABELS, lockFor } from "../../forms/signingWords";
  import { fieldLabel } from "../../forms/tabOrder";
  import Choices from "../Choices.svelte";

  interface Props {
    choices: SigningChoices;
    notes: SigningNotes | null;
    fields: FieldInfo[];
    /** only the first signature may certify */
    canCertify: boolean;
    /** the form has two or more signature fields */
    hasSigningOrder: boolean;
    lockKind: "none" | "all" | "chosen";
  }

  let { choices = $bindable(), notes, fields, canCertify, hasSigningOrder, lockKind = $bindable() }: Props = $props();

  const lockable = $derived(fields.filter((field) => field.kind !== "signature" && field.kind !== "button"));
  let locked = $state<string[]>([]);
  let certify = $state<CertificationLevel | "">("");
  const server = $derived(notes?.timestampServer.trim() ?? "");

  $effect(() => {
    choices.lock = lockFor(lockKind, locked);
  });

  $effect(() => {
    choices.certify = canCertify && certify ? certify : null;
  });

  function toggleLocked(name: string, on: boolean) {
    const rest = locked.filter((each) => each !== name);
    locked = on ? [...rest, name] : rest;
  }
</script>

<details class="options">
  <summary class="caption">Signing Options</summary>
  <div class="body">
    <label class="check">
      <input type="checkbox" bind:checked={choices.trustedTime} disabled={!server} />
      Add a Trusted Time
    </label>
    {#if !server}
      <p class="hint">To add one, choose a time stamp server under Settings, Signing. {notes?.clockTimeNote ?? ""}</p>
    {:else if choices.trustedTime}
      <!-- the engine's disclosure, word for word, before anything is sent -->
      <p class="disclosure" role="note">{notes?.timestampDisclosure}</p>
      <p class="hint">Server: {server}</p>
    {:else}
      <p class="hint">{notes?.clockTimeNote}</p>
    {/if}

    <label class="setting">
      <span class="caption">Certify</span>
      <select class="field" bind:value={certify} disabled={!canCertify}>
        <option value="">Don’t certify</option>
        {#each Object.entries(CERTIFY_LABELS) as [value, label] (value)}<option {value}>{label}</option>{/each}
      </select>
      <span class="hint">
        {#if !canCertify}
          Only the first signature can certify a document, and this one is already signed.
        {:else if certify}
          {CERTIFY_HINTS[certify]}
        {:else}
          Certifying says, as the author, which changes others may still make.
        {/if}
      </span>
    </label>

    <Choices
      label="Lock Fields After Signing"
      options={[
        { value: "none", label: "None" },
        { value: "all", label: "All" },
        { value: "chosen", label: "Chosen" },
      ]}
      bind:value={lockKind}
    />
    {#if lockKind === "chosen"}
      <div class="list">
        {#each lockable as field (field.name)}
          <label class="check">
            <input type="checkbox" checked={locked.includes(field.name)} onchange={(event) => toggleLocked(field.name, event.currentTarget.checked)} />
            {fieldLabel(field)}
          </label>
        {/each}
      </div>
    {/if}
    {#if lockKind !== "none"}<p class="hint">Locked fields become read-only. Signature fields stay open, so others can still sign.</p>{/if}

    {#if hasSigningOrder}
      <label class="check"><input type="checkbox" bind:checked={choices.followSigningOrder} /> Follow the signing order</label>
      <p class="hint">Refuses to sign a field while an earlier one in the order (Prepare Form, Signers) is still empty.</p>
    {/if}

    <label class="check"><input type="checkbox" bind:checked={choices.validationData} /> Store the certificates for later checking</label>
    <label class="check"><input type="checkbox" bind:checked={choices.auditTrail} /> Record it in the audit trail as {notes?.actor ?? "you"}</label>
    {#if choices.auditTrail && notes}<p class="hint">{notes.auditTrailNote}</p>{/if}
  </div>
</details>

<style>
  .options summary {
    cursor: default;
  }

  .body {
    display: flex;
    flex-direction: column;
    gap: 0.5rem;
    margin-top: 0.5rem;
  }

  .disclosure {
    margin: 0;
    padding: 0.375rem 0.5rem;
    border-radius: var(--radius-small);
    background: var(--control);
    font: var(--subheadline);
    line-height: 0.9375rem;
  }

  .list {
    display: flex;
    flex-direction: column;
    gap: 0.25rem;
    max-height: 8rem;
    overflow: auto;
  }
</style>

<script lang="ts">
  // Sign with a Certificate: a digital signature that lets others check
  // the file hasn't changed since it was signed. choose a certificate file
  // (.p12 or .pfx) and type its password, or make a self-signed one here;
  // choose where the signature shows (an empty signature field, a place
  // drawn on the page, or nowhere); add a reason and a place if wanted.
  //
  // the signature covers the saved file, so unsaved changes are saved
  // first. the password is used for this one signature and never kept.
  import { untrack } from "svelte";
  import { signEffect } from "../../forms/fillSignEffects";
  import { chooseCertificate, type SignaturePlacement } from "../../forms/formsApi";
  import { fileNameOf } from "../../pageApi";
  import { fieldLabel } from "../../forms/tabOrder";
  import type { ToolContext } from "../../inspector";
  import Choices from "../Choices.svelte";
  import ToolSection from "../ToolSection.svelte";
  import CertificateSheet from "./CertificateSheet.svelte";

  interface Props {
    context: ToolContext;
    open: boolean;
    onToggle: () => void;
  }

  let { context, open, onToggle }: Props = $props();

  const session = $derived(context.fillSign!);
  const emptyFields = $derived(session.fields.filter((field) => field.kind === "signature" && !field.signed));
  let certificatePath = $state<string | null>(null);
  let password = $state("");
  let where = $state<"field" | "draw" | "invisible">("draw");
  let fieldName = $state("");
  let reason = $state("");
  let location = $state("");
  let withPicture = $state(false);
  let makingCertificate = $state(false);
  const drawingPlace = $derived(session.tool === "signArea");

  // a signature field clicked on the page, or the first empty one, is the natural place
  $effect(() => {
    const requested = session.requestedSignField;
    const first = emptyFields[0]?.name;
    untrack(() => {
      if (requested) {
        where = "field";
        fieldName = requested;
      } else if (first && !fieldName) {
        where = "field";
        fieldName = first;
      }
    });
  });

  // closing the section puts its tool away
  let wasOpen = false;
  $effect(() => {
    const isOpen = open;
    untrack(() => {
      if (wasOpen && !isOpen && session.tool === "signArea") session.setTool("none");
    });
    wasOpen = isOpen;
  });

  function placement(): SignaturePlacement | null {
    if (where === "invisible") return { type: "invisible" };
    if (where === "field") return fieldName ? { type: "existingField", field: fieldName } : null;
    return session.signArea ? { type: "newField", page: session.signArea.page, rect: session.signArea.rect } : null;
  }

  const missing = $derived(
    !certificatePath
      ? "Choose a certificate file, or create a self-signed one."
      : !password
        ? "Type the certificate’s password."
        : !placement()
          ? where === "draw"
            ? "Draw where the signature goes, or choose Invisible."
            : "Choose a signature field."
          : null,
  );

  function drawPlace(event: MouseEvent) {
    // from the keyboard: the usual size in the middle of the view
    if (event.detail === 0) return session.signAreaFromKeyboard();
    session.setTool(drawingPlace ? "none" : "signArea");
  }

  async function choose() {
    const path = await chooseCertificate();
    if (path) certificatePath = path;
  }

  async function sign() {
    const chosen = placement();
    if (!certificatePath || !chosen || !(await session.saveBeforeSigning())) return;
    const signed = await session.sign({
      certificatePath,
      password,
      placement: chosen,
      reason: reason || null,
      location: location || null,
      signatureId: withPicture ? session.chosenPicture : null,
      showDetails: true,
    });
    if (signed) {
      // the password is forgotten as soon as it has been used
      password = "";
      context.notify("Signed and saved. Changing the document now would make the signature show as changed.");
    }
  }
</script>

<ToolSection
  tool="signWithCertificate"
  {open}
  {onToggle}
  actionLabel={context.document.dirty ? "Save and Sign" : "Sign"}
  ready={missing === null}
  effect={signEffect(context.document.dirty, missing === null, missing)}
  onAction={sign}
>
  <div class="setting">
    <span class="caption">Certificate</span>
    {#if certificatePath}<p class="hint">“{fileNameOf(certificatePath)}”</p>{/if}
    <div class="row">
      <button class="button" onclick={choose}>{certificatePath ? "Choose Another…" : "Choose Certificate…"}</button>
      <button class="button" onclick={() => (makingCertificate = true)}>Create a Self-Signed Certificate…</button>
    </div>
  </div>
  <label class="setting">
    <span class="caption">Password</span>
    <input class="field" type="password" autocomplete="off" bind:value={password} />
    <span class="hint">Used for this signature only. Letters never keeps it.</span>
  </label>
  <Choices
    label="Where It Shows"
    options={[
      { value: "field", label: "Field", disabled: emptyFields.length === 0, title: emptyFields.length === 0 ? "This document has no empty signature fields" : undefined },
      { value: "draw", label: "Draw" },
      { value: "invisible", label: "Invisible" },
    ]}
    bind:value={where}
  />
  {#if where === "field"}
    <label class="setting">
      <span class="caption">Signature Field</span>
      <select class="field" bind:value={fieldName}>
        {#each emptyFields as field (field.name)}<option value={field.name}>{fieldLabel(field)}</option>{/each}
      </select>
    </label>
  {:else if where === "draw"}
    <div class="row">
      <button class="button" aria-pressed={drawingPlace} onclick={drawPlace}>{drawingPlace ? "Stop Drawing" : session.signArea ? "Draw Again" : "Draw the Place"}</button>
      {#if session.signArea}<span class="hint">On page {session.signArea.page + 1}.</span>{/if}
    </div>
  {:else}
    <p class="hint">Nothing shows on the page; the signature can still be checked under Signatures.</p>
  {/if}
  {#if where !== "invisible" && session.picture}
    <label class="check"><input type="checkbox" bind:checked={withPicture} /> Show my signature picture in it</label>
  {/if}
  <label class="setting">
    <span class="caption">Reason (optional)</span>
    <input class="field" placeholder="I agree to these terms" bind:value={reason} />
  </label>
  <label class="setting">
    <span class="caption">Place (optional)</span>
    <input class="field" placeholder="Toronto" bind:value={location} />
  </label>
</ToolSection>

{#if makingCertificate}
  <CertificateSheet
    onDone={(made, typedPassword) => {
      makingCertificate = false;
      if (made) {
        certificatePath = made.path;
        password = typedPassword;
      }
    }}
    onError={context.showError}
  />
{/if}

<style>
  .row {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 0.375rem;
  }
</style>

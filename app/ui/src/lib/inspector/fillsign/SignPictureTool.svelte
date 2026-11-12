<script lang="ts">
  // Sign with a Picture: make a picture of a signature (draw it, type it
  // in a handwriting style, or choose a photo of it), keep it on this
  // computer for next time, and place it on the page: a click puts it
  // there at its usual size (or inside an empty signature field), a drag
  // chooses the size, and it keeps its proportions. kept signatures live
  // in the app's own folder, never in a document, and can be deleted.
  import { untrack } from "svelte";
  import { annotationAuthor } from "../../annotate/annotateApi";
  import { placeSignatureEffect } from "../../forms/fillSignEffects";
  import ImageSignature from "../../forms/ImageSignature.svelte";
  import SignaturePad from "../../forms/SignaturePad.svelte";
  import TypedSignature from "../../forms/TypedSignature.svelte";
  import type { ToolContext } from "../../inspector";
  import Choices from "../Choices.svelte";
  import ToolSection from "../ToolSection.svelte";

  interface Props {
    context: ToolContext;
    open: boolean;
    onToggle: () => void;
  }

  let { context, open, onToggle }: Props = $props();

  const session = $derived(context.fillSign!);
  const placing = $derived(session.tool === "placeSignature");
  let how = $state<"draw" | "type" | "image">("draw");
  let author = $state("");

  // the kept signatures and the name to type are read once, when first needed
  let loaded = false;
  $effect(() => {
    if (!open || loaded) return;
    loaded = true;
    untrack(() => {
      session.loadPictures();
      annotationAuthor()
        .then((name) => (author = name))
        .catch(() => {});
    });
  });

  // closing the section puts its tool away
  let wasOpen = false;
  $effect(() => {
    const isOpen = open;
    untrack(() => {
      if (wasOpen && !isOpen && session.tool === "placeSignature") session.setTool("none");
    });
    wasOpen = isOpen;
  });

  function act(event: MouseEvent) {
    if (event.detail === 0) return session.placeSignatureFromKeyboard();
    session.setTool(placing ? "none" : "placeSignature");
  }

  const keep = (kind: "drawn" | "typed" | "image") => (png: Uint8Array) => session.keepPicture(png, kind);
</script>

<ToolSection
  tool="signWithPicture"
  {open}
  {onToggle}
  actionLabel={placing ? "Stop Placing" : "Place Signature"}
  pressed={placing}
  ready={session.picture !== null}
  effect={placeSignatureEffect(session.picture !== null, placing)}
  onAction={act}
>
  {#if session.pictures.length > 0}
    <fieldset class="kept">
      <legend class="caption">Your Signatures</legend>
      {#each session.pictures as picture (picture.id)}
        <div class="kept-row" class:chosen={session.chosenPicture === picture.id}>
          <label>
            <input type="radio" name="kept-signature" value={picture.id} bind:group={session.chosenPicture} />
            <img src={picture.picture} alt={`${picture.kind === "typed" ? "Typed" : picture.kind === "drawn" ? "Drawn" : "Picture of a"} signature`} />
          </label>
          <button class="button delete" title="Delete this signature from this computer" onclick={() => session.deletePicture(picture.id)}>Delete</button>
        </div>
      {/each}
    </fieldset>
  {/if}
  <Choices
    label="Make a Signature"
    options={[
      { value: "draw", label: "Draw" },
      { value: "type", label: "Type" },
      { value: "image", label: "Picture" },
    ]}
    bind:value={how}
  />
  {#if how === "draw"}
    <SignaturePad onKeep={keep("drawn")} />
  {:else if how === "type"}
    {#key author}<TypedSignature initialName={author} onKeep={keep("typed")} />{/key}
  {:else}
    <ImageSignature onKeep={keep("image")} onError={context.showError} />
  {/if}
  <p class="hint">Signatures are kept on this computer only, for the next form.</p>
</ToolSection>

<style>
  .kept {
    display: flex;
    flex-direction: column;
    gap: 0.25rem;
    margin: 0;
    padding: 0;
    border: 0;
  }

  .kept-row {
    display: flex;
    align-items: center;
    gap: 0.375rem;
    padding: 0.25rem;
    border-radius: var(--radius-small);
    background: #ffffff;
    box-shadow: inset 0 0 0 0.5px var(--separator);
  }

  .kept-row.chosen {
    box-shadow: inset 0 0 0 1.5px var(--accent);
  }

  .kept-row label {
    position: relative;
    display: flex;
    flex: 1;
    align-items: center;
    min-width: 0;
  }

  .kept-row label:focus-within {
    outline: 3px solid var(--accent-soft);
  }

  .kept-row input {
    position: absolute;
    opacity: 0;
  }

  .kept-row img {
    max-width: 100%;
    max-height: 2.75rem;
  }

  .delete {
    padding: 0.125rem 0.5rem;
    font: var(--subheadline);
    font-weight: 500;
    color: #333333;
  }
</style>

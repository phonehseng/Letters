<script lang="ts">
  // a row of colour swatches to pick one from. it is a group of radio
  // buttons, so the arrow keys move between colours and a screen reader
  // says each colour's name.
  import type { Rgb } from "./annotateApi";
  import { rgbCss, sameColor, type Swatch } from "./palette";

  interface Props {
    label: string;
    swatches: Swatch[];
    value: Rgb | null;
    /** offer "None" first, for fills */
    allowNone?: boolean;
    disabled?: boolean;
    onChange: (color: Rgb | null) => void;
  }

  let { label, swatches, value, allowNone = false, disabled = false, onChange }: Props = $props();
  const name = `swatches-${Math.random().toString(36).slice(2)}`;
</script>

<fieldset class="setting" {disabled}>
  <legend class="caption">{label}</legend>
  <div class="swatches">
    {#if allowNone}
      <label class="swatch none" title="None">
        <input type="radio" {name} checked={value === null} onchange={() => onChange(null)} />
        <span aria-hidden="true"></span>
        <span class="visually-hidden">None</span>
      </label>
    {/if}
    {#each swatches as swatch (swatch.name)}
      <label class="swatch" title={swatch.name}>
        <input type="radio" {name} checked={sameColor(value, swatch.color)} onchange={() => onChange(swatch.color)} />
        <span aria-hidden="true" style:background={rgbCss(swatch.color)}></span>
        <span class="visually-hidden">{swatch.name}</span>
      </label>
    {/each}
  </div>
</fieldset>

<style>
  fieldset {
    min-width: 0;
    margin: 0;
    padding: 0;
    border: 0;
  }

  fieldset:disabled {
    opacity: 0.45;
  }

  .swatches {
    display: flex;
    flex-wrap: wrap;
    gap: 0.375rem;
  }

  .swatch {
    position: relative;
    display: grid;
    width: 1.375rem;
    height: 1.375rem;
  }

  input {
    position: absolute;
    inset: 0;
    margin: 0;
    opacity: 0;
  }

  .swatch span[aria-hidden] {
    border-radius: 50%;
    box-shadow: inset 0 0 0 0.5px rgb(0 0 0 / 0.25);
  }

  /* the chosen colour gets a ring, like the colour wells in Pages */
  input:checked + span {
    outline: 2px solid var(--label-secondary);
    outline-offset: 2px;
  }

  input:focus-visible + span {
    outline: 3px solid var(--accent-soft);
    box-shadow: 0 0 0 1px var(--accent);
  }

  /* "none" is a white circle with a red line through it, as in drawing apps */
  .none span[aria-hidden] {
    background: linear-gradient(
      to top right,
      #ffffff calc(50% - 1px),
      rgb(214 40 40) calc(50% - 1px),
      rgb(214 40 40) calc(50% + 1px),
      #ffffff calc(50% + 1px)
    );
  }
</style>

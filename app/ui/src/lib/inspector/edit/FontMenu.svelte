<script lang="ts" module>
  // the font picker of the Edit tab, behind one small adapter.
  //
  // existing page text can only be switched to a standard font (Helvetica,
  // Times or Courier, with their bold and italic), because those are the
  // fonts the engine can write without embedding anything. new text (Add
  // Text) may also use a font on this computer from the font library,
  // whose glyphs are copied into the file. a plain list is used for both;
  // when the font library offers its own picker it can take this one's
  // place here without the Edit tab changing.
  export interface LibraryFont {
    name: string;
    /** the font file the glyphs are copied from */
    path: string;
  }
</script>

<script lang="ts">
  import { STANDARD_FONTS } from "../../edit/cssFonts";
  import type { StandardFont } from "../../edit/editApi";

  interface Props {
    label: string;
    value: StandardFont | null;
    onChange: (font: StandardFont) => void;
    /** fonts on this computer, for new text only */
    libraryFonts?: LibraryFont[];
    libraryValue?: string | null;
    onLibraryChange?: (font: LibraryFont) => void;
  }

  let { label, value, onChange, libraryFonts = [], libraryValue = null, onLibraryChange }: Props = $props();

  const selected = $derived(libraryValue ? `library:${libraryValue}` : value ? `standard:${value}` : "");

  function choose(choice: string) {
    if (choice.startsWith("standard:")) onChange(choice.slice("standard:".length) as StandardFont);
    else if (choice.startsWith("library:")) {
      const font = libraryFonts.find((each) => each.name === choice.slice("library:".length));
      if (font) onLibraryChange?.(font);
    }
  }
</script>

<label class="setting">
  <span class="caption">{label}</span>
  <select class="field" value={selected} onchange={(event) => choose(event.currentTarget.value)}>
    {#if !selected}<option value="" disabled>Choose a font</option>{/if}
    <optgroup label="Standard fonts">
      {#each STANDARD_FONTS as font (font.font)}
        <option value={`standard:${font.font}`}>{font.label}</option>
      {/each}
    </optgroup>
    {#if libraryFonts.length > 0}
      <optgroup label="On this computer">
        {#each libraryFonts as font (font.name)}
          <option value={`library:${font.name}`}>{font.name}</option>
        {/each}
      </optgroup>
    {/if}
  </select>
</label>

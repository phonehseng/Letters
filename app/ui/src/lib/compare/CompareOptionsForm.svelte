<script lang="ts">
  // what counts as a change. the defaults suit most documents: spacing and
  // hyphenation are left out, formatting, layout and pictures are compared.
  import type { CompareOptions } from "./compareApi";

  interface Props {
    options: CompareOptions;
    onChange: (next: CompareOptions) => void;
  }

  let { options, onChange }: Props = $props();

  const CHOICES: { key: keyof CompareOptions; label: string; hint: string }[] = [
    { key: "ignoreSpacing", label: "Ignore spacing and hyphenation", hint: "“data base” and “database” count as the same." },
    { key: "compareFormatting", label: "Compare formatting", hint: "Font, size, bold, italic and colour." },
    { key: "compareLayout", label: "Compare layout", hint: "Text that moved on the page, margins, line breaks, page sizes." },
    { key: "comparePictures", label: "Compare pictures", hint: "Pictures and drawings that look different. Takes longer." },
  ];
</script>

<fieldset class="options">
  <legend class="caption">Options</legend>
  {#each CHOICES as choice (choice.key)}
    <label class="check">
      <input
        type="checkbox"
        checked={options[choice.key]}
        aria-describedby={`compare-option-${choice.key}`}
        onchange={(event) => onChange({ ...options, [choice.key]: event.currentTarget.checked })}
      />
      <span>{choice.label}</span>
    </label>
    <p class="hint" id={`compare-option-${choice.key}`}>{choice.hint}</p>
  {/each}
</fieldset>

<style>
  .options {
    display: flex;
    flex-direction: column;
    gap: 0.25rem;
    margin: 0;
    padding: 0;
    border: 0;
  }

  .hint {
    margin: 0 0 0.25rem 1.375rem;
  }
</style>

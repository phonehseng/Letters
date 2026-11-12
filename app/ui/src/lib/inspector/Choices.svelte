<script lang="ts" generics="T extends string">
  // a small segmented control for choosing one of a few settings, such
  // as Left, Right or Upside Down. it is a group of radio buttons, so the
  // arrow keys move between the choices and screen readers name it.
  interface Props {
    /** what is being chosen, read out to screen readers and shown above */
    label: string;
    /** hide the caption when the tool's own words already say it */
    hideLabel?: boolean;
    options: { value: T; label: string; disabled?: boolean; title?: string }[];
    value: T;
  }

  let { label, hideLabel = false, options, value = $bindable() }: Props = $props();
  const name = `choice-${Math.random().toString(36).slice(2)}`;
</script>

<fieldset class="setting">
  <legend class:visually-hidden={hideLabel} class="caption">{label}</legend>
  <div class="choices">
    {#each options as option (option.value)}
      <label title={option.title}>
        <input type="radio" {name} value={option.value} disabled={option.disabled} bind:group={value} />
        <span>{option.label}</span>
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

  /* outer corner = inner corner + padding, so the chosen segment nests */
  .choices {
    display: flex;
    padding: 0.125rem;
    border-radius: calc(var(--radius-small) + 0.125rem);
    background: var(--control);
  }

  label {
    position: relative;
    flex: 1;
    min-width: 0;
  }

  input {
    position: absolute;
    inset: 0;
    margin: 0;
    opacity: 0;
  }

  span {
    display: block;
    overflow: hidden;
    padding: 0.1875rem 0.25rem;
    border-radius: var(--radius-small);
    color: var(--label-secondary);
    font: var(--subheadline);
    font-weight: 500;
    letter-spacing: var(--subheadline-spacing);
    text-align: center;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  /* the chosen one is raised, not coloured: the accent stays for the action */
  input:checked + span {
    background: var(--segment-selected);
    box-shadow: var(--shadow-segment);
    color: var(--label);
  }

  input:focus-visible + span {
    outline: 3px solid var(--accent-soft);
    box-shadow: 0 0 0 1px var(--accent);
  }

  input:disabled + span {
    color: var(--label-tertiary);
  }
</style>

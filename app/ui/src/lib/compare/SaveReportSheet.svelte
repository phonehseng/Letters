<script lang="ts">
  // Save Report…: the format (a PDF with pictures of the pages and the
  // changes marked, a web page, or plain text), then the save panel.
  import Choices from "../inspector/Choices.svelte";
  import ProtectSheet from "../protect/ProtectSheet.svelte";
  import type { ReportFormat } from "./compareApi";

  interface Props {
    onSave: (format: ReportFormat, thumbnails: boolean) => void;
    onCancel: () => void;
  }

  let { onSave, onCancel }: Props = $props();

  let format = $state<ReportFormat>("pdf");
  let thumbnails = $state(true);
</script>

<ProtectSheet
  id="compare-report"
  role="dialog"
  title="Save Report"
  message="The report lists every change, page by page, each in one sentence, with a summary at the top."
  {onCancel}
>
  <Choices
    label="Save as"
    options={[
      { value: "pdf", label: "PDF" },
      { value: "html", label: "Web Page" },
      { value: "text", label: "Plain Text" },
    ]}
    bind:value={format}
  />
  {#if format === "pdf"}
    <label class="check">
      <input type="checkbox" bind:checked={thumbnails} />
      <span>Include pictures of the pages with the changes marked</span>
    </label>
  {/if}
  {#snippet buttons()}
    <button class="button" onclick={onCancel}>Cancel</button>
    <button class="button primary" data-autofocus onclick={() => onSave(format, thumbnails)}>Save…</button>
  {/snippet}
</ProtectSheet>

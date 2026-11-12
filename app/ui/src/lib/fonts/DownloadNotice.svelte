<script lang="ts">
  // the one-time notice before the first download: what leaves the
  // computer, and to whom. it is asked once; the answer is remembered.
  import ProtectSheet from "../protect/ProtectSheet.svelte";
  import { downloadNotice } from "./fontTools";

  interface Props {
    host: string;
    onCancel: () => void;
    onContinue: () => void;
  }

  let { host, onCancel, onContinue }: Props = $props();
</script>

<ProtectSheet id="font-download-notice" title="Download from the Letters Library?" message={downloadNotice(host)} {onCancel}>
  {#snippet buttons()}
    <button class="button" onclick={onCancel}>Cancel</button>
    <button class="button primary" data-autofocus onclick={onContinue}>Continue</button>
  {/snippet}
</ProtectSheet>

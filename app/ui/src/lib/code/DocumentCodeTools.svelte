<script lang="ts">
  // the two code tools the viewer and the writer share: Export as Code
  // File… and Check Code in This Document. the window (App.svelte) mounts
  // this beside an open pdf or word document; it registers both commands
  // and shows their sheets, so neither screen needs to know about code.
  // the sheets (and the checks they use) load only when first asked for,
  // so a pdf window doesn't carry them.
  import { appCommands } from "../commands";

  interface Props {
    source: { path: string; kind: "writer" | "pdf"; pdfId: number | null; edited: boolean };
  }

  let { source }: Props = $props();
  let open = $state<"export" | "check" | null>(null);

  $effect(() => {
    const takeBack = [appCommands.register("export-code-file", () => (open = "export")), appCommands.register("check-document-code", () => (open = "check"))];
    return () => takeBack.forEach((unregister) => unregister());
  });
</script>

{#if open === "export"}
  {#await import("./ExportCodeSheet.svelte") then sheet}
    <sheet.default {source} onClose={() => (open = null)} />
  {/await}
{:else if open === "check"}
  {#await import("./DocumentCodeSheet.svelte") then sheet}
    <sheet.default {source} onClose={() => (open = null)} />
  {/await}
{/if}

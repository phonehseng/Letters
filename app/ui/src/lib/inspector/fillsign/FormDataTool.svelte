<script lang="ts">
  // Form Data: a form's answers out to a file, answers in from a file
  // (each one through its field's format and checks; ones that don't fit
  // are listed with why), and many filled copies merged into one table a
  // spreadsheet opens. files stay on this computer.
  import { asCommandError } from "../../api";
  import {
    chooseDataDestination,
    chooseDataFile,
    chooseFilledCopies,
    DATA_KIND_HINTS,
    DATA_KIND_LABELS,
    dataFileBeside,
    exportData,
    importData,
    importSummary,
    mergeData,
    type DataKind,
  } from "../../forms/formDataApi";
  import type { ToolContext } from "../../inspector";
  import { pathBeside } from "../../pageApi";
  import Choices from "../Choices.svelte";
  import ToolSection from "../ToolSection.svelte";

  interface Props {
    context: ToolContext;
    open: boolean;
    onToggle: () => void;
  }

  let { context, open, onToggle }: Props = $props();

  let task = $state<"export" | "import" | "merge">("export");
  let kind = $state<DataKind>("fdf");
  let lastReport = $state<string | null>(null);

  const actionLabel = $derived({ export: "Export Answers…", import: "Import Answers…", merge: "Merge Filled Copies…" }[task]);
  const effect = $derived(
    {
      export: `Saves this form’s answers as a ${DATA_KIND_LABELS[kind]} file. The PDF doesn’t change.`,
      import: "Fills the form from an FDF, XFDF, CSV or JSON file. Each answer goes through its field’s checks; ones that don’t fit are listed. One undo step.",
      merge: "Choose several filled copies of a form. Their answers become one CSV table, a row per file, with a File column first.",
    }[task],
  );

  async function run() {
    lastReport = null;
    try {
      if (task === "export") await runExport();
      else if (task === "import") await runImport();
      else await runMerge();
    } catch (problem) {
      context.showError(asCommandError(problem).message);
    }
  }

  async function runExport() {
    const path = await chooseDataDestination(dataFileBeside(context.document.path, kind), kind);
    if (!path) return;
    await exportData(context.document.id, path);
    context.notify("Answers saved.");
  }

  async function runImport() {
    const path = await chooseDataFile();
    if (!path) return;
    const id = context.document.id;
    let summary = "";
    const worked = await context.change(async () => {
      const imported = await importData(id, path);
      summary = importSummary(imported.report);
      return imported.document;
    });
    if (worked) lastReport = summary;
  }

  async function runMerge() {
    const copies = await chooseFilledCopies();
    if (copies.length === 0) return;
    const destination = await chooseDataDestination(pathBeside(context.document.path, "Merged answers.csv"), "csv");
    if (!destination) return;
    const merged = await mergeData(copies, destination);
    const left = merged.skipped.length > 0 ? ` ${merged.skipped.length} couldn’t be read: ${merged.skipped.map((each) => each.field).join(", ")}.` : "";
    lastReport = `${merged.rows} filled ${merged.rows === 1 ? "copy" : "copies"} merged into one table of ${merged.columns} columns.${left}`;
  }
</script>

<ToolSection tool="formData" {open} {onToggle} {actionLabel} {effect} onAction={run}>
  <Choices
    label="Form Data"
    hideLabel
    options={[
      { value: "export", label: "Export" },
      { value: "import", label: "Import" },
      { value: "merge", label: "Merge" },
    ]}
    bind:value={task}
  />
  {#if task === "export"}
    <Choices label="Kind of File" options={(Object.keys(DATA_KIND_LABELS) as DataKind[]).map((value) => ({ value, label: DATA_KIND_LABELS[value] }))} bind:value={kind} />
    <p class="hint">{DATA_KIND_HINTS[kind]}</p>
  {/if}
  {#if lastReport}<p class="hint" role="status">{lastReport}</p>{/if}
</ToolSection>

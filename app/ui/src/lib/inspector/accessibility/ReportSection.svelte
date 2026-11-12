<script lang="ts">
  // Save Report…: the check's findings as an accessible web page or as
  // plain text, written by the engine.
  import { chooseA11yReportLocation, saveA11yReport } from "../../a11y/a11yApi";
  import type { A11ySession } from "../../a11y/a11ySession.svelte";
  import { asCommandError } from "../../api";
  import type { ToolContext } from "../../inspector";
  import { fileNameOf, pathBeside, stemOf } from "../../pageApi";
  import Choices from "../Choices.svelte";
  import ToolSection from "../ToolSection.svelte";

  interface Props {
    context: ToolContext;
    session: A11ySession;
    open: boolean;
    onToggle: () => void;
  }

  let { context, session, open, onToggle }: Props = $props();

  let format = $state<"html" | "text">("html");
  const extension = $derived(format === "html" ? "html" : "txt");

  async function save() {
    const suggested = pathBeside(context.document.path, `${stemOf(context.document.path)} - accessibility.${extension}`);
    const path = await chooseA11yReportLocation(suggested, format);
    if (!path) return;
    try {
      const written = await saveA11yReport(context.document.id, path, format);
      context.notify(`Saved the report as “${fileNameOf(written)}”.`);
    } catch (problem) {
      context.showError(asCommandError(problem).message);
    }
  }
</script>

<ToolSection
  tool="saveAccessibilityReport"
  {open}
  {onToggle}
  actionLabel="Save Report…"
  ready={!!session.report}
  effect={session.report ? "Checks the document again and writes what it finds to a new file. The document doesn’t change." : "Check the document first."}
  onAction={save}
>
  <Choices
    label="Save as"
    options={[
      { value: "html", label: "Web Page" },
      { value: "text", label: "Plain Text" },
    ]}
    bind:value={format}
  />
</ToolSection>

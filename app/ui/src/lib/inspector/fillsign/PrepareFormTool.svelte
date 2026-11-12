<script lang="ts">
  // Prepare Form: the tools for someone making a form rather than filling
  // one. one section with a segmented control, so the tab keeps one line
  // per tool: Rules (a field's calculation, format and checks). each pane
  // has its own one button and its own "what will happen" line.
  //
  // the form's logic is read once per revision of the document, here, and
  // handed to the panes.
  import { untrack } from "svelte";
  import { asCommandError } from "../../api";
  import { formLogic, type FormLogicView } from "../../forms/formLogicApi";
  import type { ToolContext } from "../../inspector";
  import Choices from "../Choices.svelte";
  import ToolSection from "../ToolSection.svelte";
  import RulesPane from "./RulesPane.svelte";

  interface Props {
    context: ToolContext;
    open: boolean;
    onToggle: () => void;
  }

  let { context, open, onToggle }: Props = $props();

  type Pane = "rules";
  let pane = $state<Pane>("rules");
  let logic = $state<FormLogicView | null>(null);

  // read while open, and again after every change to the document
  $effect(() => {
    const revision = context.document.revision;
    const id = context.document.id;
    if (!open) return;
    untrack(() => {
      formLogic(id)
        .then((read) => {
          if (revision === context.document.revision) logic = read;
        })
        .catch((problem) => {
          logic = null;
          context.showError(asCommandError(problem).message);
        });
    });
  });
</script>

<ToolSection tool="prepareForm" {open} {onToggle}>
  <Choices label="Prepare" hideLabel options={[{ value: "rules", label: "Rules" }]} bind:value={pane} />
  {#if pane === "rules"}
    <RulesPane {context} {logic} />
  {/if}
</ToolSection>

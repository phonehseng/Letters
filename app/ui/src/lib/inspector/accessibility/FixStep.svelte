<script lang="ts">
  // the form one step of the wizard needs, picked by the finding's fix.
  import type { A11ySession } from "../../a11y/a11ySession.svelte";
  import type { WizardStep } from "../../a11y/wizard";
  import type { ToolContext } from "../../inspector";
  import AddTagsFix from "./fixes/AddTagsFix.svelte";
  import AltTextFix from "./fixes/AltTextFix.svelte";
  import AutomaticFix from "./fixes/AutomaticFix.svelte";
  import ContrastFix from "./fixes/ContrastFix.svelte";
  import HeadingsFix from "./fixes/HeadingsFix.svelte";
  import LanguageFix from "./fixes/LanguageFix.svelte";
  import ReadingOrderFix from "./fixes/ReadingOrderFix.svelte";
  import TableHeadersFix from "./fixes/TableHeadersFix.svelte";
  import TextAnswerFix from "./fixes/TextAnswerFix.svelte";
  import TitleFix from "./fixes/TitleFix.svelte";

  interface Props {
    context: ToolContext;
    session: A11ySession;
    step: WizardStep;
  }

  let { context, session, step }: Props = $props();

  const finding = $derived(step.finding);
  const fix = $derived(finding.fix!);
</script>

{#if fix.kind === "addTags"}
  <AddTagsFix {session} />
{:else if fix.kind === "setLanguage"}
  <LanguageFix {session} suggested={fix.suggested} />
{:else if fix.kind === "setTitle"}
  <TitleFix {session} suggested={fix.suggested} />
{:else if fix.kind === "setAltText"}
  <AltTextFix {context} {session} {finding} target={fix.target} />
{:else if fix.kind === "fixHeadingLevels"}
  <HeadingsFix {context} {session} />
{:else if fix.kind === "setTableHeaders"}
  <TableHeadersFix {context} {session} {finding} {fix} />
{:else if fix.kind === "setReadingOrder"}
  <ReadingOrderFix {context} {session} page={fix.page} proposal={fix.proposal} />
{:else if fix.kind === "setFieldTooltip"}
  <TextAnswerFix
    {session}
    label="Tooltip"
    hint="A short name for the field, such as “Student ID”. Screen readers say it when the field is reached."
    suggested={fix.suggested}
    actionLabel="Set Tooltip"
    request={(text) => ({ kind: "setFieldTooltip", field: fix.field, text })}
  />
{:else if fix.kind === "addLinkText"}
  <TextAnswerFix
    {session}
    label="What the link is for"
    hint="Say where it goes, such as “Library opening hours”, not “click here”. The words on the page stay as they are."
    suggested={fix.suggested}
    actionLabel="Set Description"
    request={(text) => ({ kind: "addLinkText", annotation: fix.annotation, text })}
  />
{:else if fix.kind === "setAnnotationContents"}
  <TextAnswerFix
    {session}
    label="What the comment or mark says"
    hint="A few words a screen reader can read in its place, such as “Box around the key figure”."
    suggested={null}
    actionLabel="Set Description"
    request={(text) => ({ kind: "setAnnotationContents", annotation: fix.annotation, text })}
  />
{:else if fix.kind === "changeTextColour"}
  <ContrastFix {session} {finding} runs={fix.runs} suggested={fix.suggested} />
{:else}
  <AutomaticFix {session} {finding} {fix} />
{/if}

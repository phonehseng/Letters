<script lang="ts">
  // Make Editable Text: scanned pages become real text the Edit tab can
  // change. the person picks the pages and what becomes of the scan (text
  // over the cleaned scan, or text only), each said in one sentence. pages
  // not read yet are read first with the Recognize Text choices (language,
  // cleanup). a signed file asks first, naming the signer.
  import { untrack } from "svelte";
  import type { ToolContext } from "../../inspector";
  import { editableEffect, editableSentence, progressSentence, SCAN_MODES } from "../../ocr/ocrSentences";
  import { availableChoices, defaultChoice, pagesFor, type PageChoice } from "../../ocr/pageRange";
  import Choices from "../Choices.svelte";
  import ToolSection from "../ToolSection.svelte";

  interface Props {
    context: ToolContext;
    open: boolean;
    onToggle: () => void;
  }

  let { context, open, onToggle }: Props = $props();

  const session = $derived(context.scan!);
  const pageCount = $derived(context.document.pages.length);
  const facts = $derived({
    current: context.currentPage,
    selection: context.target.fromSelection ? context.target.pages : [],
    pageCount,
    scanned: session.overview?.scanned ?? [],
  });
  const choices = $derived(availableChoices(facts));
  let choice = $state<PageChoice>("current");
  const pages = $derived(pagesFor(choice, facts));
  const unread = $derived(pages.filter((page) => !(session.overview?.recognized ?? []).includes(page)).length);
  const mode = $derived(SCAN_MODES.find((each) => each.value === session.editableMode)!);
  const needsLanguage = $derived(unread > 0 && session.languages.length === 0);

  $effect(() => {
    session.overview?.scanned.length;
    untrack(() => {
      choice = defaultChoice(facts);
      if (!choices.some((each) => each.value === choice)) choice = choices[0].value;
    });
  });

  async function act() {
    // the edit tab's question for signed files guards this change too
    const consent = context.edit?.consent;
    if (consent && !(await consent.ensure())) return;
    await session.makeEditable(pages, consent?.consent ?? { invalidateSignatures: false });
  }
</script>

<ToolSection
  tool="makeEditableText"
  {open}
  {onToggle}
  actionLabel="Make Editable Text"
  ready={!session.busy && pages.length > 0 && !needsLanguage && (unread === 0 || session.status?.available === true)}
  effect={session.busy ? progressSentence(session.view) : needsLanguage ? "Choose the document’s language under Recognize Text first." : editableEffect(pages, pageCount, unread)}
  onAction={act}
>
  <Choices label="Pages" options={choices} bind:value={choice} />
  <Choices label="The scan" options={SCAN_MODES} bind:value={session.editableMode} />
  <p class="hint">{mode.sentence}</p>
  <p class="hint">Letters writes the words exactly as they were read, in Helvetica, Times or Courier, whichever is closest to the scan, at each word’s place and width.</p>
  {#if session.editable}
    <p class="result" role="status">{editableSentence(session.editable)}</p>
  {/if}
</ToolSection>

<style>
  .result {
    margin: 0;
  }
</style>

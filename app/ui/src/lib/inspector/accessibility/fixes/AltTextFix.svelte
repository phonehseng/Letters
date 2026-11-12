<script lang="ts">
  // alt text for one picture: the picture itself, a sentence on what good
  // alt text is, a text field, and "Decorative", which tells screen
  // readers to skip a picture that adds nothing (a border, a flourish).
  import type { Finding, Target } from "../../../a11y/a11yApi";
  import type { A11ySession } from "../../../a11y/a11ySession.svelte";
  import type { ToolContext } from "../../../inspector";
  import RegionPicture from "./RegionPicture.svelte";

  interface Props {
    context: ToolContext;
    session: A11ySession;
    finding: Finding;
    target: Target;
  }

  let { context, session, finding, target }: Props = $props();

  let text = $state("");
  let busy = $state(false);
  const fieldId = $derived(`alt-text-${finding.where.page}-${Math.round(finding.where.rect?.x ?? 0)}`);

  async function run(request: Parameters<A11ySession["fix"]>[0]) {
    busy = true;
    await session.fix(request);
    busy = false;
  }
</script>

{#if finding.where.page !== null && finding.where.rect}
  <RegionPicture
    documentId={context.document.id}
    revision={context.document.revision}
    page={finding.where.page}
    rect={finding.where.rect}
    label={`The picture on page ${finding.where.page + 1}`}
  />
{/if}
<p class="hint">{session.words?.altText ?? ""} Say what the picture shows and why it is there, in a sentence or two.</p>
<label class="setting" for={fieldId}>
  <span class="caption">Alt text</span>
</label>
<textarea id={fieldId} class="field" rows="3" bind:value={text} placeholder="such as A map of the Rhine from the Alps to the North Sea"></textarea>
<div class="actions">
  <button class="button" disabled={busy} onclick={() => run({ kind: "markAsArtifact", target })}>Decorative — Skip for Screen Readers</button>
  <button class="button primary" disabled={!text.trim() || busy} onclick={() => run({ kind: "setAltText", target, text: text.trim() })}>Save Alt Text</button>
</div>

<style>
  textarea {
    width: 100%;
    resize: vertical;
  }

  .actions {
    display: flex;
    flex-wrap: wrap;
    justify-content: flex-end;
    gap: 0.375rem;
  }
</style>

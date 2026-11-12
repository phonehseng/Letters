<script lang="ts">
  // the Protect sheets of one window: the review, the apply question and
  // the question before unapplied marks would be lost. the viewer puts
  // this in once; each sheet shows while the session asks for it.
  import type { DocumentState } from "../api";
  import ApplySheet from "./ApplySheet.svelte";
  import DiscardMarksSheet from "./DiscardMarksSheet.svelte";
  import type { ProtectSession } from "./protectSession.svelte";
  import ReviewSheet from "./ReviewSheet.svelte";

  interface Props {
    session: ProtectSession;
    document: DocumentState;
  }

  let { session, document: doc }: Props = $props();
</script>

{#if session.leaveQuestion}
  <DiscardMarksSheet count={session.leaveQuestion.count} onAnswer={session.leaveQuestion.answer} />
{:else if session.confirmingApply}
  <ApplySheet {session} pageCount={doc.pages.length} />
{:else if session.review}
  <ReviewSheet {session} document={doc} />
{/if}

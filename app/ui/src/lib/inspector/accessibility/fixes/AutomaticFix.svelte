<script lang="ts">
  // a fix that needs no answer: one sentence on what it does and one button.
  import type { Finding, FixKind } from "../../../a11y/a11yApi";
  import type { A11ySession } from "../../../a11y/a11ySession.svelte";
  import type { FixRequest } from "../../../a11y/a11yApi";

  interface Props {
    session: A11ySession;
    finding: Finding;
    fix: FixKind;
  }

  let { session, finding, fix }: Props = $props();

  let busy = $state(false);

  const LABELS: Partial<Record<FixKind["kind"], string>> = {
    markAsTagged: "Mark as Tagged",
    showTitle: "Show the Title",
    setTabOrder: "Follow the Tags",
    tagAnnotations: "Tag Links and Comments",
    tagUntaggedContent: "Tag This Page’s Content",
    addBookmarksFromHeadings: "Add Bookmarks",
  };

  /** the request for this kind of fix, which carries nothing but the page at most. */
  function requestFor(kind: FixKind): FixRequest | null {
    switch (kind.kind) {
      case "markAsTagged":
      case "showTitle":
      case "setTabOrder":
      case "tagAnnotations":
      case "addBookmarksFromHeadings":
        return { kind: kind.kind };
      case "tagUntaggedContent":
        return { kind: "tagUntaggedContent", page: kind.page };
      default:
        return null;
    }
  }

  const request = $derived(requestFor(fix));

  async function apply() {
    if (!request) return;
    busy = true;
    await session.fix(request);
    busy = false;
  }
</script>

<p class="hint">{finding.fixDescription}</p>
{#if request}
  <div class="actions">
    <button class="button primary" disabled={busy} onclick={apply}>{LABELS[fix.kind] ?? "Fix"}</button>
  </div>
{:else}
  <p class="hint">Letters can’t fix this one here. Skip it for now.</p>
{/if}

<style>
  .actions {
    display: flex;
    justify-content: flex-end;
  }
</style>

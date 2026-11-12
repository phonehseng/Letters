<script lang="ts">
  // flattening: painting comments and marks into the pages for good. the
  // explanation is the engine's own sentence, and the line under the
  // button says plainly that this can't be edited afterwards but that
  // Undo still works until the document is closed.
  import { flattenAnnotations, flattenExplanation } from "../../annotate/annotateApi";
  import { flattenEffect } from "../../annotate/annotateEffects";
  import type { ToolContext } from "../../inspector";
  import { tools } from "../../tools";
  import Choices from "../Choices.svelte";
  import ToolSection from "../ToolSection.svelte";

  interface Props {
    context: ToolContext;
    open: boolean;
    onToggle: () => void;
  }

  let { context, open, onToggle }: Props = $props();

  let scope = $state<"page" | "all">("all");
  let explanation = $state(tools.flattenComments.meaning);
  flattenExplanation()
    .then((sentence) => (explanation = sentence))
    .catch(() => {});

  const session = $derived(context.annotate);
  const page = $derived(context.currentPage);
  // links and form fields are left alone by flattening, so they aren't counted
  const count = $derived(
    session.annotations.filter((annotation) => annotation.kind !== "other" && (scope === "all" || annotation.page === page)).length,
  );

  async function flatten() {
    const id = context.document.id;
    const chosen = scope;
    const pages = [page];
    const worked = await context.change(async () => {
      const edit = await flattenAnnotations(id, chosen === "all" ? { scope: "allPages" } : { scope: "pages", pages });
      return edit.document;
    });
    if (worked) {
      session.deselect();
      context.notify(chosen === "all" ? "Comments and marks are now part of every page." : `Comments and marks are now part of page ${page + 1}.`);
    }
  }
</script>

<ToolSection
  tool="flattenComments"
  {open}
  {onToggle}
  actionLabel="Flatten"
  ready={count > 0}
  effect={flattenEffect(count, scope === "all" ? "all" : [page])}
  onAction={flatten}
>
  {#if explanation !== tools.flattenComments.meaning}
    <p class="hint">{explanation}</p>
  {/if}
  <Choices
    label="Flatten"
    options={[
      { value: "page", label: `Page ${page + 1}` },
      { value: "all", label: "All Pages" },
    ]}
    bind:value={scope}
  />
</ToolSection>

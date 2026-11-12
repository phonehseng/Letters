<script lang="ts">
  // the name other readers see on comments and marks. it starts as the
  // name used to sign in to the computer; a blank field goes back to that.
  import { annotationAuthor, setAnnotationAuthor } from "../../annotate/annotateApi";
  import type { ToolContext } from "../../inspector";
  import ToolSection from "../ToolSection.svelte";

  interface Props {
    context: ToolContext;
    open: boolean;
    onToggle: () => void;
  }

  let { open, onToggle }: Props = $props();

  let name = $state("");
  let saved = $state("");
  annotationAuthor()
    .then((current) => {
      name = current;
      saved = current;
    })
    .catch(() => {});

  async function save() {
    if (name.trim() === saved) return;
    saved = await setAnnotationAuthor(name);
    name = saved;
  }
</script>

<ToolSection
  tool="authorName"
  {open}
  {onToggle}
  effect={saved ? `New comments and marks say they are by “${saved}”.` : ""}
>
  <label class="setting">
    <span class="caption">Name</span>
    <input
      class="field"
      type="text"
      autocomplete="name"
      bind:value={name}
      onchange={save}
      onkeydown={(event) => event.key === "Enter" && save()}
    />
  </label>
</ToolSection>

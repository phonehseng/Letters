<script lang="ts">
  // Add Text: real text written into the page, in the style of the text
  // nearby (its own font, size, spacing and colour), or in a standard font
  // or a font on this computer (its glyphs are copied into the file). click
  // where it should start, then type on the page; the field is the text's
  // own size, colour and font, so what is typed is what is added.
  import { untrack } from "svelte";
  import { INK_SWATCHES } from "../../annotate/palette";
  import Swatches from "../../annotate/Swatches.svelte";
  import { STANDARD_FONTS } from "../../edit/cssFonts";
  import type { StandardFont } from "../../edit/editApi";
  import { ADD_TEXT_DIFFERENCE, addTextEffect } from "../../edit/editSentences";
  import { fontsApi } from "../../fonts/fontsApi";
  import type { ToolContext } from "../../inspector";
  import ToolSection from "../ToolSection.svelte";
  import FontMenu, { type LibraryFont } from "./FontMenu.svelte";

  interface Props {
    context: ToolContext;
    open: boolean;
    onToggle: () => void;
  }

  let { context, open, onToggle }: Props = $props();

  const session = $derived(context.edit!);
  const active = $derived(session.tool === "addText");
  const style = $derived(session.newText);
  let libraryFonts = $state<LibraryFont[]>([]);
  let libraryLoaded = false;

  // closing the section puts its tool away
  let wasOpen = false;
  $effect(() => {
    const isOpen = open;
    untrack(() => {
      if (wasOpen && !isOpen && session.tool === "addText") session.setTool("none");
      if (isOpen && !libraryLoaded) loadLibrary();
    });
    wasOpen = isOpen;
  });

  /** fonts on this computer that allow being copied into a pdf, as TrueType files. */
  async function loadLibrary() {
    libraryLoaded = true;
    try {
      const { families } = await fontsApi.families();
      const usable = families.filter((family) => family.onThisComputer && family.embeddable && !family.variable).slice(0, 400);
      libraryFonts = usable.map((family) => ({ name: family.name, path: "" }));
    } catch {
      // without the font library, the standard fonts are still there
      libraryFonts = [];
    }
  }

  function chooseStandard(font: StandardFont) {
    const label = STANDARD_FONTS.find((each) => each.font === font)?.label ?? "Helvetica";
    session.newText = { ...style, font: { type: "standard", font }, label };
  }

  async function chooseLibrary(font: LibraryFont) {
    try {
      const details = await fontsApi.details(font.name);
      const file = details.files.find((path) => /\.ttf$/i.test(path));
      if (!file) {
        context.showError(`${font.name} can’t be copied into a PDF from here, because its file isn’t TrueType (.ttf). Choose another font.`);
        return;
      }
      session.newText = { ...style, font: { type: "file", path: file }, label: font.name };
    } catch (problem) {
      context.showError(String((problem as { message?: string })?.message ?? problem));
    }
  }

  function setSize(value: number) {
    if (Number.isFinite(value) && value >= 1 && value <= 400) session.newText = { ...style, size: value };
  }

  function act(event: MouseEvent) {
    if (event.detail === 0) {
      // from the keyboard: the text starts on the page being read, ready to type
      session.setTool("addText");
      const point = viewPoint();
      if (point) session.startNewText(point.page, point);
      return;
    }
    session.setTool(active ? "none" : "addText");
  }

  function viewPoint() {
    const page = context.currentPage;
    const size = context.document.pages[page];
    return size ? { page, x: size.width * 0.15, y: size.height * 0.4 } : null;
  }
</script>

<ToolSection
  tool="addPageText"
  {open}
  {onToggle}
  actionLabel={active ? "Stop Adding" : "Add Text"}
  pressed={active}
  effect={addTextEffect(active, context.currentPage)}
  onAction={act}
>
  <label class="check">
    <input type="checkbox" bind:checked={session.matchNearby} />
    <span>Match the text nearby</span>
  </label>
  <p class="hint">
    {session.matchNearby
      ? "New text takes the font, size, spacing and colour of the nearest paragraph. Letters its font lacks are added to that same font, and Letters says where they came from."
      : "New text uses the font, size and colour chosen below."}
  </p>
  {#if !session.matchNearby}
    <FontMenu
      label="Font"
      value={style.font.type === "standard" ? style.font.font : null}
      onChange={chooseStandard}
      {libraryFonts}
      libraryValue={style.font.type === "file" ? style.label : null}
      onLibraryChange={chooseLibrary}
    />
    <label class="setting">
      <span class="caption">Size</span>
      <input class="field" type="number" min="1" max="400" step="0.5" value={style.size} onchange={(event) => setSize(Number(event.currentTarget.value))} />
    </label>
    <Swatches label="Colour" swatches={INK_SWATCHES} value={style.color} onChange={(color) => color && (session.newText = { ...style, color })} />
  {/if}
  <p class="hint">{ADD_TEXT_DIFFERENCE}</p>
</ToolSection>

<script lang="ts">
  // the Format tab: its sections follow the selection, as in Pages. text
  // and paragraph settings for text; table settings when the cursor is in
  // a table; size, alt text and wrap when a picture is selected.
  import { tools, tooltipFor } from "../../tools";
  import * as edit from "../editing";
  import { documentFonts, fontChoices, SAFE_FONTS } from "../fonts";
  import { FontPicker } from "../../fonts";
  import { imageWrap, placementFor } from "../editing";
  import { schema } from "../schema";
  import { choiceForParagraph, documentStyleChoices, STANDARD_STYLES, styleHeadingLevel } from "../styles";
  import { formatLength, lengthToTwips, lengthValue, pointsToTwips, twipsToPoints, type LengthUnit } from "../units";
  import WriterButton from "../WriterButton.svelte";
  import type { WriterSession } from "../writerSession.svelte";
  import { isBulletList } from "../numbering";

  interface Props {
    session: WriterSession;
    unit: LengthUnit;
  }

  let { session, unit }: Props = $props();

  const editorState = $derived(session.editor);
  const image = $derived(edit.selectedImage(editorState));
  const current = $derived(edit.currentParagraphProps(editorState));
  const para = $derived(current?.props ?? {});
  const inTable = $derived(session.inTable());
  const meta = $derived(session.meta);
  const styleKey = $derived(
    current ? choiceForParagraph(meta.styles, current.data.styleId, current.node.type.name === "heading" ? (current.node.attrs.level as number) : current.data.styleId ? styleHeadingLevel(meta.styles, current.data.styleId) : null) : "body",
  );
  const list = $derived(current ? edit.listOf(meta, current.node) : null);
  const listKind = $derived(list ? (isBulletList(meta.numbering, list.listId) ? "bullet" : "number") : "none");
  const fontData = $derived(edit.markData(editorState, schema.marks.fontFamily));
  const sizeData = $derived(edit.markData(editorState, schema.marks.fontSize));
  const colourData = $derived(edit.markData(editorState, schema.marks.color));
  const highlightData = $derived(edit.markData(editorState, schema.marks.highlight));
  const lines = $derived(para.spacing?.line?.rule === "auto" ? para.spacing.line.value / 240 : 1);
  let systemFonts = $state<string[]>(SAFE_FONTS);
  const fonts = $derived(fontChoices(documentFonts(meta, fontData?.name ? [String(fontData.name)] : []), systemFonts));

  const active = (name: string) => edit.markActive(editorState, schema.marks[name]);
  const run = (make: Parameters<WriterSession["run"]>[0]) => session.run(make, false);

  function setFont(name: string) {
    run((s) => edit.setMark(s, "fontFamily", name ? { name } : null));
  }

  function setSize(points: number) {
    if (points > 0 && points < 1000) run((s) => edit.setMark(s, "fontSize", { halfPoints: Math.round(points * 2) }));
  }

  function setLength(change: (twips: number) => void, value: string) {
    const number = Number.parseFloat(value);
    if (Number.isFinite(number)) change(lengthToTwips(number, unit));
  }

  function setIndentField(field: "left" | "right" | "firstLine" | "hanging", value: string) {
    setLength((twips) =>
      run((s) =>
        edit.setIndent(s, (indent) => {
          const next = { ...indent, [field]: twips };
          // first line and hanging are two views of one value
          if (field === "firstLine") delete next.hanging;
          if (field === "hanging") delete next.firstLine;
          return next;
        }),
      ),
    value);
  }

  function setList(kind: string) {
    if (kind === "none" && list) run((s) => edit.toggleList(s, listKind as "bullet" | "number"));
    else if (kind !== "none" && kind !== listKind) run((s) => edit.toggleList(s, kind as "bullet" | "number"));
  }

  // pictures
  const imageData = $derived(image ? (image.node.attrs.data as Record<string, unknown>) : null);
  const imageWidth = $derived(Number(imageData?.width ?? 0));
  const imageHeight = $derived(Number(imageData?.height ?? 0));

  function resizeImage(widthValue: string) {
    const width = lengthToTwips(Number.parseFloat(widthValue), unit);
    if (!Number.isFinite(width) || width <= 0 || !imageWidth) return;
    const ratio = imageHeight / imageWidth;
    run((s) => edit.updateImage(s, (data) => ({ ...data, width, height: Math.round(width * ratio) })));
  }

  const BORDER = { style: "single", size: 4 };
  function setBorders(kind: "all" | "outside" | "none") {
    const none = { style: "none", size: 0 };
    const outside = kind === "none" ? none : BORDER;
    const inside = kind === "all" ? BORDER : none;
    session.updateTable((data) => ({ ...data, borders: { top: outside, left: outside, bottom: outside, right: outside, insideHorizontal: inside, insideVertical: inside } }));
  }

  const firstRowIsHeader = $derived.by(() => {
    const here = editorState.selection.$from;
    for (let depth = here.depth; depth > 0; depth -= 1) {
      const node = here.node(depth);
      if (node.type.name === "table") return !!(node.firstChild?.attrs.data as { header?: boolean })?.header;
    }
    return false;
  });
</script>

{#if image && imageData}
  <section aria-labelledby="w-picture">
    <h3 id="w-picture">Picture</h3>
    <div class="setting">
      <span class="caption" title={tooltipFor("wImageSize")}>{tools.wImageSize.label}</span>
      <div class="setting-row">
        <label class="setting">
          <span class="caption">Width ({unit})</span>
          <input class="field" type="number" step="0.1" min="0.1" value={lengthValue(imageWidth, unit)} onchange={(event) => resizeImage(event.currentTarget.value)} />
        </label>
        <div class="setting">
          <span class="caption">Height ({unit})</span>
          <span class="readonly">{lengthValue(imageHeight, unit)}</span>
        </div>
      </div>
      <p class="hint">The height follows the width, so the picture keeps its proportions.</p>
    </div>
    <label class="setting">
      <span class="caption" title={tooltipFor("wImageAltText")}>{tools.wImageAltText.label}</span>
      <textarea
        class="field"
        rows="3"
        placeholder="Describe what the picture shows"
        value={String(imageData.description ?? "")}
        onchange={(event) => {
          const description = event.currentTarget.value;
          run((s) => edit.updateImage(s, (data) => ({ ...data, description })));
        }}
      ></textarea>
      <p class="hint">{tools.wImageAltText.meaning}</p>
    </label>
    <label class="setting">
      <span class="caption" title={tooltipFor("wImageWrap")}>{tools.wImageWrap.label}</span>
      <select class="field" value={imageWrap(imageData)} onchange={async (event) => {
        const wrap = event.currentTarget.value;
        const wasInline = imageWrap(imageData) === "inline";
        // in line and free go through the engine, so the picture doesn't jump
        if (wrap === "inline" || wasInline) await session.objects.setImageFree(wrap !== "inline");
        if (wrap !== "inline") run((s) => edit.updateImage(s, (data) => ({ ...data, placement: placementFor(wrap, (data.placement ?? {}) as Record<string, unknown>) })));
      }}>
        <option value="inline">In Line with Text</option>
        <option value="square">Square: text around its box</option>
        <option value="tight">Tight: drawn as square here; Word wraps closer</option>
        <option value="topAndBottom">Top and Bottom</option>
        <option value="behindText">Behind Text</option>
        <option value="inFrontOfText">In Front of Text</option>
      </select>
    </label>
  </section>
{:else}
  <section aria-labelledby="w-text">
    <h3 id="w-text">Text</h3>
    <label class="setting">
      <span class="caption" title={tooltipFor("wStyle")}>{tools.wStyle.label}</span>
      <select class="field" value={styleKey} onchange={(event) => session.applyStyle(event.currentTarget.value)}>
        {#each STANDARD_STYLES as choice (choice.key)}<option value={choice.key}>{choice.label}</option>{/each}
        {#each documentStyleChoices(meta.styles) as choice (choice.key)}<option value={choice.key}>{choice.label}</option>{/each}
      </select>
    </label>
    <div class="setting-row">
      <div class="setting wide">
        <span class="caption" title={tooltipFor("wFont")}>{tools.wFont.label}</span>
        <!-- the font library's picker (lib/fonts); a font it adds or downloads is already loaded into the editor -->
        <FontPicker value={String(fontData?.name ?? "")} documentFonts={fonts.find((group) => group.group === "In This Document")?.names ?? []} label={tools.wFont.label} onChange={(family) => setFont(family)} />
        {#if fontData?.name}
          <button class="link" title="Use the paragraph style’s font again" onclick={() => setFont("")}>Use the Style’s Font</button>
        {/if}
      </div>
      <label class="setting">
        <span class="caption" title={tooltipFor("wFontSize")}>{tools.wFontSize.label} (pt)</span>
        <input class="field" type="number" min="1" max="999" step="0.5" value={sizeData ? Number(sizeData.halfPoints) / 2 : ""} placeholder="Style" onchange={(event) => setSize(Number(event.currentTarget.value))} />
      </label>
    </div>
    <div class="buttons" role="group" aria-label="Character formatting">
      <WriterButton name="wBold" icon="bold" compact pressed={active("bold")} onClick={() => session.command("format-bold")} />
      <WriterButton name="wItalic" icon="italic" compact pressed={active("italic")} onClick={() => session.command("format-italic")} />
      <WriterButton name="wUnderline" icon="underline" compact pressed={active("underline")} onClick={() => session.command("format-underline")} />
      <WriterButton name="wStrike" icon="strike" compact pressed={active("strike")} onClick={() => session.command("format-strike")} />
      <button class="small" class:on={active("superscript")} title={tooltipFor("wSuperscript")} aria-pressed={active("superscript")} onclick={() => session.command("format-superscript")}>x²</button>
      <button class="small" class:on={active("subscript")} title={tooltipFor("wSubscript")} aria-pressed={active("subscript")} onclick={() => session.command("format-subscript")}>x₂</button>
    </div>
    <div class="setting-row">
      <label class="setting">
        <span class="caption" title={tooltipFor("wTextColour")}>{tools.wTextColour.label}</span>
        <input class="field colour" type="color" value={colourData ? `#${colourData.hex}` : "#000000"} onchange={(event) => {
          const hex = event.currentTarget.value.slice(1).toUpperCase();
          run((s) => edit.setMark(s, "color", hex === "000000" ? null : { hex }));
        }} />
      </label>
      <label class="setting">
        <span class="caption" title={tooltipFor("wHighlight")}>{tools.wHighlight.label}</span>
        <select class="field" value={String(highlightData?.color ?? "")} onchange={(event) => {
          const color = event.currentTarget.value;
          run((s) => edit.setMark(s, "highlight", color ? { color } : null));
        }}>
          <option value="">None</option>
          <option value="yellow">Yellow</option>
          <option value="green">Green</option>
          <option value="cyan">Blue</option>
          <option value="magenta">Pink</option>
          <option value="lightGray">Grey</option>
        </select>
      </label>
    </div>
  </section>

  <section aria-labelledby="w-paragraph">
    <h3 id="w-paragraph">Paragraph</h3>
    <div class="segments" role="radiogroup" aria-label={tools.wAlignment.label} title={tooltipFor("wAlignment")}>
      {#each [["left", "alignLeft", "Left"], ["center", "alignCenter", "Centre"], ["right", "alignRight", "Right"], ["justify", "alignJustify", "Justify"]] as const as [value, , label] (value)}
        <button class="segment" class:selected={(para.alignment ?? "left") === value} role="radio" aria-checked={(para.alignment ?? "left") === value} onclick={() => session.command(`align-${value}` as const)}>{label}</button>
      {/each}
    </div>
    <div class="setting-row">
      <label class="setting">
        <span class="caption" title={tooltipFor("wLineSpacing")}>{tools.wLineSpacing.label}</span>
        <select class="field" value={String(Math.round(lines * 100) / 100)} onchange={(event) => run((s) => edit.setLineSpacing(s, Number(event.currentTarget.value)))}>
          {#each [1, 1.08, 1.15, 1.5, 2, 2.5, 3] as value (value)}<option value={String(value)}>{value === 1 ? "Single" : value === 2 ? "Double" : value}</option>{/each}
          {#if ![1, 1.08, 1.15, 1.5, 2, 2.5, 3].includes(Math.round(lines * 100) / 100)}<option value={String(Math.round(lines * 100) / 100)}>{Math.round(lines * 100) / 100}</option>{/if}
        </select>
      </label>
      <label class="setting">
        <span class="caption" title={tooltipFor("wSpacing")}>Before (pt)</span>
        <input class="field" type="number" min="0" step="1" value={twipsToPoints(para.spacing?.before ?? 0)} onchange={(event) => run((s) => edit.setSpacing(s, "before", pointsToTwips(Number(event.currentTarget.value))))} />
      </label>
      <label class="setting">
        <span class="caption" title={tooltipFor("wSpacing")}>After (pt)</span>
        <input class="field" type="number" min="0" step="1" value={twipsToPoints(para.spacing?.after ?? 0)} onchange={(event) => run((s) => edit.setSpacing(s, "after", pointsToTwips(Number(event.currentTarget.value))))} />
      </label>
    </div>
    <div class="setting">
      <span class="caption" title={tooltipFor("wIndents")}>{tools.wIndents.label} ({unit})</span>
      <div class="setting-row">
        <label class="setting"><span class="caption">Left</span><input class="field" type="number" step="0.1" value={lengthValue(para.indent?.left ?? 0, unit)} onchange={(event) => setIndentField("left", event.currentTarget.value)} /></label>
        <label class="setting"><span class="caption">Right</span><input class="field" type="number" step="0.1" value={lengthValue(para.indent?.right ?? 0, unit)} onchange={(event) => setIndentField("right", event.currentTarget.value)} /></label>
        <label class="setting"><span class="caption">First Line</span><input class="field" type="number" step="0.1" min="0" value={lengthValue(para.indent?.firstLine ?? 0, unit)} onchange={(event) => setIndentField("firstLine", event.currentTarget.value)} /></label>
        <label class="setting"><span class="caption">Hanging</span><input class="field" type="number" step="0.1" min="0" value={lengthValue(para.indent?.hanging ?? 0, unit)} onchange={(event) => setIndentField("hanging", event.currentTarget.value)} /></label>
      </div>
      <p class="hint">{tools.wIndents.meaning} Tab at the start of a paragraph adds a {formatLength(720, unit)} first-line indent.</p>
    </div>
    <div class="setting">
      <span class="caption" title={tooltipFor("wLists")}>{tools.wLists.label}</span>
      <div class="segments" role="radiogroup" aria-label={tools.wLists.label}>
        {#each [["none", "None"], ["bullet", "Bullets"], ["number", "Numbers"]] as const as [value, label] (value)}
          <button class="segment" class:selected={listKind === value} role="radio" aria-checked={listKind === value} onclick={() => setList(value)}>{label}</button>
        {/each}
      </div>
      {#if list}
        <div class="buttons">
          <WriterButton name="wIndents" icon="indentLess" compact onClick={() => session.command("indent-less")} />
          <span class="level">Level {list.level + 1}</span>
          <WriterButton name="wIndents" icon="indentMore" compact onClick={() => session.command("indent-more")} />
        </div>
      {/if}
    </div>
  </section>

  {#if inTable}
    <section aria-labelledby="w-table">
      <h3 id="w-table">Table</h3>
      <div class="setting">
        <span class="caption" title={tooltipFor("wTableRows")}>{tools.wTableRows.label}</span>
        <div class="buttons wrap">
          <button class="button" onclick={() => session.table("rowBefore")}>Add Above</button>
          <button class="button" onclick={() => session.table("rowAfter")}>Add Below</button>
          <button class="button" onclick={() => session.table("deleteRow")}>Delete</button>
        </div>
      </div>
      <div class="setting">
        <span class="caption" title={tooltipFor("wTableColumns")}>{tools.wTableColumns.label}</span>
        <div class="buttons wrap">
          <button class="button" onclick={() => session.table("columnBefore")}>Add Before</button>
          <button class="button" onclick={() => session.table("columnAfter")}>Add After</button>
          <button class="button" onclick={() => session.table("deleteColumn")}>Delete</button>
        </div>
      </div>
      <div class="setting">
        <span class="caption" title={tooltipFor("wTableMerge")}>{tools.wTableMerge.label}</span>
        <div class="buttons wrap">
          <button class="button" onclick={() => session.table("merge")}>Merge Cells</button>
          <button class="button" onclick={() => session.table("split")}>Split Cell</button>
        </div>
        <p class="hint">To merge, drag across cells first.</p>
      </div>
      <div class="setting">
        <span class="caption" title={tooltipFor("wTableBorders")}>{tools.wTableBorders.label}</span>
        <div class="segments">
          <button class="segment" onclick={() => setBorders("all")}>All</button>
          <button class="segment" onclick={() => setBorders("outside")}>Outside</button>
          <button class="segment" onclick={() => setBorders("none")}>None</button>
        </div>
      </div>
      <label class="setting">
        <span class="caption" title={tooltipFor("wTableShading")}>{tools.wTableShading.label}</span>
        <select class="field" onchange={(event) => {
          const value = event.currentTarget.value;
          session.updateCells((data) => {
            const next = { ...data };
            if (value) next.shading = value;
            else delete next.shading;
            return next;
          });
        }}>
          <option value="">None</option>
          <option value="D9D9D9">Light Grey</option>
          <option value="DEEAF6">Light Blue</option>
          <option value="E2EFD9">Light Green</option>
          <option value="FFF2CC">Light Yellow</option>
        </select>
      </label>
      <label class="check" title={tooltipFor("wTableHeaderRow")}>
        <input type="checkbox" checked={firstRowIsHeader} onchange={(event) => session.setHeaderRow(event.currentTarget.checked)} />
        {tools.wTableHeaderRow.label}
      </label>
    </section>
  {/if}
{/if}

<style>
  .link {
    align-self: flex-start;
    margin-top: 0.25rem;
    padding: 0;
    border: 0;
    background: transparent;
    color: var(--accent-label);
    font: var(--footnote);
  }

  section {
    display: flex;
    flex-direction: column;
    gap: 0.625rem;
    padding: 0.75rem 0.25rem;
    border-top: 0.5px solid var(--separator);
  }

  section:first-child {
    border-top: 0;
  }

  h3 {
    margin: 0;
    font: var(--headline);
  }

  .setting-row {
    grid-template-columns: repeat(auto-fit, minmax(4rem, 1fr));
  }

  .wide {
    grid-column: span 2;
  }

  .buttons {
    display: flex;
    align-items: center;
    gap: 0.25rem;
  }

  .buttons.wrap {
    flex-wrap: wrap;
  }

  .small {
    width: 2rem;
    height: 1.75rem;
    border: 0;
    border-radius: var(--radius-small);
    background: var(--control);
    font: var(--body);
  }

  .small.on {
    background: var(--accent-soft);
    color: var(--accent-label);
  }

  .level {
    min-width: 4rem;
    color: var(--label-secondary);
    font: var(--subheadline);
    text-align: center;
  }

  .readonly {
    padding: 0.3125rem 0;
    color: var(--label-secondary);
  }

  textarea.field {
    resize: vertical;
    font: var(--body);
  }

  .colour {
    height: 1.75rem;
    padding: 0.125rem;
  }

  .segments {
    display: flex;
    padding: 0.125rem;
    border-radius: calc(var(--radius-small) + 0.125rem);
    background: var(--control);
  }

  .segment {
    flex: 1;
    min-width: 0;
    height: 1.5rem;
    border: 0;
    border-radius: var(--radius-small);
    background: transparent;
    color: var(--label-secondary);
    font: var(--subheadline);
    font-weight: 500;
  }

  .segment.selected {
    background: var(--segment-selected);
    box-shadow: var(--shadow-segment);
    color: var(--label);
  }
</style>

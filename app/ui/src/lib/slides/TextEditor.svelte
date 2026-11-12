<script lang="ts">
  // typing in place: a field laid exactly over a text box, in its theme's
  // font, size, colour, alignment and spacing, while the slide underneath
  // is drawn without that text (shell-core slides/pictures.rs, LeftOut),
  // so only these words show. Escape, a click elsewhere or Ctrl Enter
  // keeps the words as one undo step, as powerpoint and keynote do.
  import { onMount, untrack } from "svelte";
  import { slideFace } from "./slideFonts";
  import type { ObjectView, ParagraphView } from "./slidesApi";

  interface Props {
    object: ObjectView;
    deckId: number;
    /** css pixels per point */
    scale: number;
    /** words typed to start editing (a key pressed with the box selected) replace the text */
    startWith?: string | null;
    onDone: (text: string | null) => void;
  }

  let { object, deckId, scale, startWith = null, onDone }: Props = $props();

  let field = $state<HTMLDivElement>();
  let finished = false;
  // read once: the field owns its words from here on
  const text = untrack(() => object.text!);
  const original = text.paragraphs.map((paragraph) => paragraph.text).join("\n");
  const paragraphs: ParagraphView[] = untrack(() => (startWith === null ? text.paragraphs : [{ ...text.paragraphs[0], text: startWith }]));
  const [areaLeft, areaTop, areaRight, areaBottom] = text.area;
  const justify = { top: "flex-start", middle: "center", bottom: "flex-end" }[text.anchor];

  /** each paragraph's face as the engine draws it, once it has loaded */
  let faces = $state<(string | null)[]>([]);

  function family(paragraph: ParagraphView, index: number): string {
    const face = faces[index];
    return face ? `'${face}', '${paragraph.font}', sans-serif` : `'${paragraph.font}', sans-serif`;
  }

  function lineHeight(paragraph: ParagraphView): string {
    // powerpoint's line pitch is 1.2 times the size times the spacing (core/slides layout/lines.rs)
    if (paragraph.lineHeight.type === "points") return `${paragraph.lineHeight.value * scale}px`;
    return `${1.2 * paragraph.lineHeight.value}`;
  }

  /** the words now, one paragraph per line. */
  function wordsNow(): string {
    if (!field) return original;
    // deleting everything can leave bare text in the field, outside any paragraph
    const looseText = [...field.childNodes].some((node) => node.nodeType === Node.TEXT_NODE && node.textContent);
    if (looseText) return field.innerText.replace(/\n+$/, "");
    const lines = [...field.children].map((child) => (child as HTMLElement).innerText.replace(/\n$/, ""));
    return lines.join("\n");
  }

  function finish() {
    if (finished) return;
    finished = true;
    const words = wordsNow();
    onDone(words === original ? null : words);
  }

  function onKeyDown(event: KeyboardEvent) {
    if (event.key === "Escape" || (event.key === "Enter" && (event.ctrlKey || event.metaKey))) {
      event.preventDefault();
      finish();
    }
    // levels are kept from the file; Tab doesn't change them here
    if (event.key === "Tab") event.preventDefault();
  }

  onMount(() => {
    void Promise.all(paragraphs.map((paragraph) => slideFace(deckId, paragraph.font, paragraph.bold, paragraph.italic))).then((found) => (faces = found));
    field?.focus();
    // the caret goes to the end, as a click into powerpoint's box does
    const range = document.createRange();
    if (field) {
      range.selectNodeContents(field);
      range.collapse(false);
      const selection = window.getSelection();
      selection?.removeAllRanges();
      selection?.addRange(range);
    }
  });
</script>

<div
  class="box"
  style:left="{object.x * scale}px"
  style:top="{object.y * scale}px"
  style:width="{object.width * scale}px"
  style:height="{object.height * scale}px"
  style:transform="rotate({object.rotation}deg)"
>
  <div
    class="area"
    style:padding="{areaTop * scale}px {(object.width - areaRight) * scale}px {(object.height - areaBottom) * scale}px {areaLeft * scale}px"
    style:justify-content={justify}
  >
    <div
      bind:this={field}
      class="typing"
      class:nowrap={!text.wrap}
      contenteditable="plaintext-only"
      role="textbox"
      tabindex="0"
      aria-multiline="true"
      aria-label="Text of {object.name}. Escape keeps it."
      spellcheck="true"
      onkeydown={onKeyDown}
      onblur={finish}
      onpointerdown={(event) => event.stopPropagation()}
    >
      {#each paragraphs as paragraph, index (index)}
        <div
          class="paragraph"
          data-bullet={paragraph.bullet && paragraph.bullet.length === 1 ? paragraph.bullet : null}
          style:font-family={family(paragraph, index)}
          style:font-size="{paragraph.size * text.fontScale * scale}px"
          style:font-weight={paragraph.bold ? 700 : 400}
          style:font-style={paragraph.italic ? "italic" : "normal"}
          style:text-decoration={paragraph.underline ? "underline" : "none"}
          style:color={paragraph.color}
          style:text-align={paragraph.align}
          style:line-height={lineHeight(paragraph)}
          style:padding-left="{paragraph.marginLeft * scale}px"
          style:text-indent="{paragraph.indent * scale}px"
          style:margin-top="{index === 0 ? 0 : paragraph.spaceBefore * scale}px"
          style:margin-bottom="{paragraph.spaceAfter * scale}px"
          style:--hang="{Math.abs(paragraph.indent) * scale}px"
        >{paragraph.text}</div>
      {/each}
    </div>
  </div>
</div>

<style>
  .box {
    position: absolute;
    z-index: 3;
    /* the accent frame says "this is being typed into" */
    outline: 1px solid var(--accent);
  }

  .area {
    display: flex;
    flex-direction: column;
    box-sizing: border-box;
    width: 100%;
    height: 100%;
  }

  .typing {
    min-height: 1em;
    outline: none;
    cursor: text;
    white-space: pre-wrap;
    overflow-wrap: break-word;
  }

  .typing.nowrap {
    white-space: pre;
  }

  .paragraph {
    min-height: 1em;
  }

  /* the bullet hangs in the indent, as the slide draws it; it isn't part of the words */
  .paragraph[data-bullet]::before {
    content: attr(data-bullet);
    display: inline-block;
    min-width: var(--hang);
    text-indent: 0;
  }
</style>

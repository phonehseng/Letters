<script lang="ts">
  // typing straight into the page's text. the field sits exactly over the
  // paragraph (editorGeometry.ts), in the paragraph's own font when the web
  // view can load it, at its size and colour, so nothing jumps when typing
  // starts. it is nearly white so the old words don't show through.
  //
  // clicking elsewhere or Ctrl Enter (Command Return) makes the change, as
  // one undo step; Escape drops it. copy and paste are plain text, as in
  // any text field. a letter the font can't draw is said under the field
  // before anything changes.
  import { tick, untrack } from "svelte";
  import type { PageFrame } from "../pageTools/pagePoints";
  import { leadingRun } from "./blocks";
  import { rgbCss } from "./cssFonts";
  import { editorBox, newTextBox } from "./editorGeometry";
  import type { EditSession } from "./editSession.svelte";
  import { typingHint } from "./editSentences";
  import { missingCharacters } from "./textDiff";

  interface Props {
    session: EditSession;
    frame: PageFrame;
  }

  let { session, frame }: Props = $props();

  const draft = $derived(session.editor!);
  let text = $state(untrack(() => session.editor?.text ?? ""));
  let field = $state<HTMLTextAreaElement>();
  let widthPx = $state(0);
  let done = false;

  const lineCount = $derived(Math.max(1, text.split("\n").length));
  const run = $derived(draft.block ? leadingRun(draft.block) : null);
  const box = $derived.by(() => {
    if (draft.block) return editorBox(draft.block, frame, draft.metrics, lineCount);
    const place = newTextBox(draft.position, session.newText.size, frame, draft.metrics);
    return { ...place, width: 12 * frame.scale, height: lineCount * place.lineHeight, rotation: 0, origin: { x: 0, y: 0 } };
  });
  const color = $derived(run ? rgbCss(run.color) : rgbCss(session.newText.color));
  const hint = $derived(run ? typingHint(missingCharacters(text, draft.original, run.font.editableChars), run.font) : "");
  const align = $derived(draft.block?.alignment === "center" ? "center" : draft.block?.alignment === "right" ? "right" : "left");

  $effect(() => {
    tick().then(() => {
      if (!field) return;
      field.focus({ preventScroll: true });
      const caret = Math.min(untrack(() => draft.caret), field.value.length);
      field.setSelectionRange(caret, caret);
      grow();
    });
  });

  /** the field is as wide as its longest line, never narrower than the paragraph. */
  function grow() {
    if (!field) return;
    widthPx = 0;
    widthPx = field.scrollWidth;
  }

  function report() {
    if (!field) return;
    session.typed(text, [field.selectionStart, field.selectionEnd]);
  }

  async function commit() {
    if (done) return;
    done = true;
    report();
    await session.commitEditor();
  }

  function cancel() {
    done = true;
    session.cancelEditor();
  }

  function onKeyDown(event: KeyboardEvent) {
    if (event.key === "Escape") {
      event.preventDefault();
      event.stopPropagation();
      cancel();
      return;
    }
    if (event.key === "Enter" && (event.ctrlKey || event.metaKey)) {
      event.preventDefault();
      commit();
    }
    // the page commands (delete pages, move pages) must not hear typing
    event.stopPropagation();
  }

  /** leaving the field makes the change, unless the focus went to the inspector's style controls. */
  function onFocusOut(event: FocusEvent) {
    const next = event.relatedTarget as HTMLElement | null;
    if (next?.closest(".inspector") || next?.closest("[data-edit-notice]")) return;
    commit();
  }
</script>

<textarea
  bind:this={field}
  bind:value={text}
  class="in-place"
  data-page-ui
  wrap="off"
  spellcheck="true"
  aria-label={draft.block
    ? "Editing the text on the page. Press Control Enter to finish, or Escape to cancel."
    : "New text for the page. Type it, then press Control Enter to finish, or Escape to cancel."}
  aria-describedby={hint ? "in-place-hint" : undefined}
  style:left="{box.left}px"
  style:top="{box.top}px"
  style:width="{Math.max(box.width, widthPx)}px"
  style:height="{box.height}px"
  style:font-family={draft.look.family}
  style:font-weight={draft.look.weight}
  style:font-style={draft.look.style}
  style:font-size="{box.fontSize}px"
  style:line-height="{box.lineHeight}px"
  style:color
  style:text-align={align}
  style:transform={box.rotation ? `rotate(${box.rotation}deg)` : undefined}
  style:transform-origin="{box.origin.x}px {box.origin.y}px"
  oninput={() => (grow(), report())}
  onselect={report}
  onkeyup={report}
  onkeydown={onKeyDown}
  onfocusout={onFocusOut}
  onpointerdown={(event) => event.stopPropagation()}
></textarea>

{#if hint}
  <p
    id="in-place-hint"
    class="hint glass"
    data-page-ui
    style:left="{box.left}px"
    style:top="{box.top + box.height + 4}px"
  >
    {hint}
  </p>
{/if}

<style>
  .in-place {
    position: absolute;
    z-index: 3;
    box-sizing: content-box;
    margin: 0;
    padding: 0;
    overflow: hidden;
    border: 0;
    border-radius: 0;
    outline: 1px solid var(--accent);
    outline-offset: 1px;
    box-shadow: none;
    /* nearly white, so the words underneath don't show through */
    background: rgb(255 255 255 / 0.96);
    resize: none;
    letter-spacing: 0;
    white-space: pre;
    font-kerning: normal;
  }

  .in-place:focus-visible {
    outline: 1px solid var(--accent);
    box-shadow: 0 0 0 3px var(--accent-soft);
  }

  .hint {
    position: absolute;
    z-index: 3;
    max-width: 22rem;
    margin: 0;
    padding: 0.375rem 0.625rem;
    font: var(--subheadline);
    letter-spacing: var(--subheadline-spacing);
  }
</style>

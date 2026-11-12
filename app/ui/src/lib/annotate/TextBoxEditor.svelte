<script lang="ts">
  // typing a text box straight onto the page. the field sits exactly
  // where the box will be and uses its font, size, colour and alignment,
  // so what is typed looks like the result. clicking elsewhere on the
  // page or pressing Ctrl Enter (Command Return) makes it the annotation;
  // Escape drops what was typed. the box grows downward as lines are added.
  import { tick, untrack } from "svelte";
  import type { PageFrame, Rect } from "../pageTools/pagePoints";
  import { cssFont, standardFont } from "./annotateEffects";
  import type { AnnotateSession } from "./annotateSession.svelte";
  import { rgbCss } from "./palette";

  interface Props {
    session: AnnotateSession;
    frame: PageFrame;
  }

  let { session, frame }: Props = $props();

  /** the engine leaves this much space inside a box's edge, in points */
  const PADDING_POINTS = 2;
  /** the engine's line spacing, as a share of the font size */
  const LINE_SPACING = 1.2;

  const draft = $derived(session.editing!);
  const style = $derived(session.styles.textBox);
  const font = $derived(cssFont(standardFont(style.family, style.bold, style.italic)));

  // the field starts with the box's words; the overlay makes a new editor for another box
  let text = $state(untrack(() => session.editing?.text ?? ""));
  let height = $state(untrack(() => session.editing?.rect.height ?? 36));
  let field = $state<HTMLTextAreaElement>();
  let done = false;

  $effect(() => {
    tick().then(() => {
      field?.focus();
      const end = field?.value.length ?? 0;
      field?.setSelectionRange(end, end);
    });
  });

  /** the box as it will be saved: the place it was given, as tall as its text needs. */
  function finalRect(): Rect {
    const room = frame.size.height - draft.rect.y;
    return { ...draft.rect, height: Math.min(Math.max(height, draft.rect.height), room) };
  }

  function grow() {
    if (!field) return;
    // the text's own height, back in points
    const needed = field.scrollHeight / frame.scale;
    if (needed > height) height = needed;
  }

  async function commit() {
    if (done) return;
    done = true;
    await session.commitTextBox(text, finalRect());
  }

  function cancel() {
    done = true;
    session.editing = null;
  }

  function onKeyDown(event: KeyboardEvent) {
    if (event.key === "Escape") {
      event.preventDefault();
      event.stopPropagation();
      cancel();
    } else if (event.key === "Enter" && (event.ctrlKey || event.metaKey)) {
      event.preventDefault();
      commit();
    }
    // the page commands (delete pages, move pages) must not hear typing
    event.stopPropagation();
  }

  /** leaving the field ends the typing, unless the focus went to the text box's own controls. */
  function onFocusOut(event: FocusEvent) {
    const next = event.relatedTarget as HTMLElement | null;
    if (next?.closest(".inspector")) return;
    commit();
  }
</script>

<textarea
  bind:this={field}
  bind:value={text}
  class="editor"
  data-page-ui
  aria-label="Text box. Type the text; press Control Enter to finish, or Escape to cancel."
  spellcheck="true"
  style:left="{frame.left + draft.rect.x * frame.scale}px"
  style:top="{frame.top + draft.rect.y * frame.scale}px"
  style:width="{draft.rect.width * frame.scale}px"
  style:height="{Math.max(height, draft.rect.height) * frame.scale}px"
  style:padding="{PADDING_POINTS * frame.scale}px"
  style:font-family={font.family}
  style:font-weight={font.weight}
  style:font-style={font.style}
  style:font-size="{style.size * frame.scale}px"
  style:line-height={LINE_SPACING}
  style:color={rgbCss(style.color)}
  style:text-align={style.align}
  style:background={style.fill ? rgbCss(style.fill) : draft.id ? "rgb(255 255 255 / 0.94)" : "transparent"}
  oninput={grow}
  onkeydown={onKeyDown}
  onfocusout={onFocusOut}
  onpointerdown={(event) => event.stopPropagation()}
></textarea>

<style>
  .editor {
    position: absolute;
    z-index: 2;
    margin: 0;
    overflow: hidden;
    border: 0;
    border-radius: 0;
    outline: 1px dashed var(--accent);
    outline-offset: 0;
    box-shadow: none;
    resize: none;
    letter-spacing: 0;
    white-space: pre-wrap;
    overflow-wrap: break-word;
  }

  .editor:focus-visible {
    outline: 1px dashed var(--accent);
    box-shadow: 0 0 0 3px var(--accent-soft);
  }
</style>

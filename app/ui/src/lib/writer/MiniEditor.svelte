<script lang="ts">
  // a small editor for a header, a footer or a footnote: the same schema,
  // keys and typing rules as the page, on its own few paragraphs. every
  // change is handed up as model blocks, and the page's editor stores them
  // in the document, so the main Undo covers them too.
  import { EditorState } from "prosemirror-state";
  import { EditorView } from "prosemirror-view";
  import { onMount } from "svelte";
  import { blocksToDoc, docToBlocks } from "./convert";
  import { imageView, type MediaCache } from "./images";
  import type { Block } from "./model";
  import { writerPlugins } from "./plugins";
  import type { Replacement, TypingSettings } from "./typing";

  interface Props {
    blocks: Block[];
    label: string;
    placeholder?: string;
    typing: () => TypingSettings;
    spellcheck: boolean;
    language: string;
    media: MediaCache;
    onChange: (blocks: Block[]) => void;
    onFocus?: (view: EditorView) => void;
    onAutocorrect: (change: Replacement) => void;
  }

  let { blocks, label, placeholder = "", typing, spellcheck, language, media, onChange, onFocus, onAutocorrect }: Props = $props();

  let host = $state<HTMLDivElement>();
  let view: EditorView | null = null;
  // the blocks this editor last handed up, to tell its own changes from outside ones (undo)
  let lastSent = "";

  function stateFor(next: Block[]): EditorState {
    return EditorState.create({
      doc: blocksToDoc(next),
      plugins: writerPlugins({ typing, onAutocorrect, onPreservedRefused: () => {} }),
    });
  }

  onMount(() => {
    if (!host) return;
    lastSent = JSON.stringify(blocks);
    view = new EditorView(host, {
      state: stateFor(blocks),
      nodeViews: { image: imageView(media) },
      attributes: { role: "textbox", "aria-multiline": "true", "aria-label": label, class: "w-doc w-mini", "data-placeholder": placeholder },
      dispatchTransaction(tr) {
        if (!view) return;
        view.updateState(view.state.apply(tr));
        if (tr.docChanged) {
          const next = docToBlocks(view.state.doc);
          lastSent = JSON.stringify(next);
          onChange(next);
        }
      },
      handleDOMEvents: {
        focus: (editor) => {
          onFocus?.(editor);
          return false;
        },
      },
    });
    return () => view?.destroy();
  });

  // an outside change (undo in the page, a preset) replaces what is shown
  $effect(() => {
    const incoming = JSON.stringify(blocks);
    if (view && incoming !== lastSent) {
      lastSent = incoming;
      view.updateState(stateFor(blocks));
    }
  });

  $effect(() => {
    view?.dom.setAttribute("spellcheck", String(spellcheck));
    view?.dom.setAttribute("lang", language);
  });

  export function focus() {
    view?.focus();
  }
</script>

<div class="mini" bind:this={host}></div>

<style>
  .mini :global(.ProseMirror) {
    outline: none;
    min-height: 1.2em;
  }

  .mini :global(.ProseMirror[data-placeholder]:has(> p:only-child:empty))::before {
    content: attr(data-placeholder);
    position: absolute;
    color: var(--w-placeholder, #9a9aa0);
    pointer-events: none;
  }
</style>

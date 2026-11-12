<script lang="ts">
  // the editor itself: mounts codemirror once, loads the language when it
  // changes, and sets the code colours for light and dark mode. everything
  // else (saving, checking, panels) belongs to CodeScreen.
  import type { EditorView } from "@codemirror/view";
  import { onDestroy, onMount } from "svelte";
  import { createEditor, setIndentation, setLanguage, setWrap, type EditorEvents, type EditorOptions } from "./editorSetup";
  import { loadLanguage } from "./languageLoader";
  import type { LanguageId } from "./languages";
  import type { Indentation } from "./textFormat";

  interface Props {
    options: EditorOptions;
    events: EditorEvents;
    language: LanguageId;
    wrap: boolean;
    indentation: Indentation;
    /** the codemirror view, for the screen's commands */
    view?: EditorView;
  }

  let { options, events, language, wrap, indentation, view = $bindable() }: Props = $props();
  let host = $state<HTMLDivElement>();

  onMount(() => {
    if (host) view = createEditor(host, options, events);
    view?.focus();
  });

  onDestroy(() => view?.destroy());

  // a language loads in the background; typing goes on meanwhile. only
  // the newest request is applied, so switching twice quickly is safe
  let languageRequest = 0;
  $effect(() => {
    const wanted = language;
    const editor = view;
    if (!editor || !options.fullFeatures) return;
    const request = ++languageRequest;
    loadLanguage(wanted).then((support) => {
      if (request === languageRequest) setLanguage(editor, support);
    });
  });

  $effect(() => {
    if (view) setWrap(view, wrap);
  });

  $effect(() => {
    if (view) setIndentation(view, indentation);
  });
</script>

<div class="editor" bind:this={host}></div>

<style>
  .editor {
    height: 100%;
    /* the code colours, chosen to pass contrast on the code background in both modes */
    --code-background: #ffffff;
    --code-font: ui-monospace, "Cascadia Mono", "SF Mono", Menlo, Consolas, monospace;
    --code-keyword: rgb(155 35 147);
    --code-string: rgb(196 26 22);
    --code-comment: rgb(93 108 121);
    --code-number: rgb(28 0 207);
    --code-type: rgb(11 79 121);
    --code-function: rgb(50 109 116);
    --code-tag: rgb(136 18 128);
    --code-attribute: rgb(153 69 0);
    --code-meta: rgb(100 56 32);
  }

  @media (prefers-color-scheme: dark) {
    .editor {
      --code-background: #1f1f21;
      --code-keyword: rgb(252 95 163);
      --code-string: rgb(252 106 93);
      --code-comment: rgb(127 140 152);
      --code-number: rgb(208 191 105);
      --code-type: rgb(93 216 255);
      --code-function: rgb(103 183 164);
      --code-tag: rgb(252 95 163);
      --code-attribute: rgb(253 143 63);
      --code-meta: rgb(253 143 63);
    }
  }

  .editor :global(.cm-editor.cm-focused) {
    outline: none;
  }
</style>

<script lang="ts" module>
  // the writer's symbols, drawn like the app's own (Icon.svelte): thin
  // rounded strokes on a 24 by 24 grid in the current text colour.
  const paths = {
    style: "M5 6.5h14 M5 11h9 M5 15.5h14 M5 20h7",
    insert: "M12 5v14 M5 12h14",
    table: "M4.5 5.5h15v13h-15Z M4.5 10h15 M4.5 14.5h15 M9.5 5.5v13 M14.5 5.5v13",
    picture: "M4.5 6h15v12h-15Z M4.5 15.5l4.5-4.5 3.5 3.5 2.5-2.5 4.5 4.5 M15.5 9.2h.01",
    comment: "M5 6.5h14v9H11l-4 3v-3H5Z",
    format: "M7 18 12 6l5 12 M8.8 13.8h6.4",
    review: "M5 17.5 15.5 7l1.5 1.5L6.5 19H5Z M13.5 9l1.5 1.5 M12 19h7",
    document: "M7 4.5h7l3.5 3.5v11.5H7Z M14 4.5V8h3.5",
    ruler: "M3.5 9h17v6h-17Z M7 9v2.5 M10.5 9v2 M14 9v2.5 M17.5 9v2",
    bold: "M8 5.5h5a3.2 3.2 0 0 1 0 6.4H8Z M8 11.9h6a3.3 3.3 0 0 1 0 6.6H8Z",
    italic: "M10 5.5h6 M8 18.5h6 M14 5.5l-4 13",
    underline: "M8 5v6a4 4 0 0 0 8 0V5 M6.5 19h11",
    strike: "M5.5 12h13 M16 7.5c-.6-1.4-2-2.2-4-2.2-2.4 0-4 1.2-4 3 0 1.3.9 2.2 2.5 2.8 M8 16.5c.6 1.4 2.2 2.2 4.2 2.2 2.4 0 4-1.2 4-3",
    alignLeft: "M5 6.5h14 M5 10.5h9 M5 14.5h14 M5 18.5h9",
    alignCenter: "M5 6.5h14 M7.5 10.5h9 M5 14.5h14 M7.5 18.5h9",
    alignRight: "M5 6.5h14 M10 10.5h9 M5 14.5h14 M10 18.5h9",
    alignJustify: "M5 6.5h14 M5 10.5h14 M5 14.5h14 M5 18.5h14",
    bullets: "M9.5 7h10 M9.5 12h10 M9.5 17h10 M5.5 7h.01 M5.5 12h.01 M5.5 17h.01",
    numbers: "M10 7h9.5 M10 12h9.5 M10 17h9.5 M5 6l1-.8v3.5 M4.8 13.5h2 M4.8 10.8h1.3a.7.7 0 0 1 0 1.4l-1.3 1.3 M4.8 15.6h1.4a.7.7 0 0 1 0 1.4H5.4h.8a.7.7 0 0 1 0 1.4H4.8",
    indentMore: "M11 6.5h8 M11 10.5h8 M11 14.5h8 M5 18.5h14 M5 8l3 2.5L5 13",
    indentLess: "M11 6.5h8 M11 10.5h8 M11 14.5h8 M5 18.5h14 M8 8l-3 2.5L8 13",
    check: "M5.5 12.5l4 4 9-9",
    chevronDown: "M6.5 9.5 12 15l5.5-5.5",
    up: "M12 18.5v-13 M6.5 11 12 5.5l5.5 5.5",
    down: "M12 5.5v13 M6.5 13 12 18.5l5.5-5.5",
    replace: "M5 9h11l-3-3 M19 15H8l3 3",
    trash: "M6 7.5h12 M9.5 7.5V5.5h5v2 M7.5 7.5l.8 11h7.4l.8-11",
  };

  export type WriterIconName = keyof typeof paths;
</script>

<script lang="ts">
  interface Props {
    name: WriterIconName;
    size?: number;
  }
  let { name, size = 1.25 }: Props = $props();
</script>

<svg
  viewBox="0 0 24 24"
  width="{size}rem"
  height="{size}rem"
  fill="none"
  stroke="currentColor"
  stroke-width="1.5"
  stroke-linecap="round"
  stroke-linejoin="round"
  aria-hidden="true"
>
  <path d={paths[name]} />
</svg>

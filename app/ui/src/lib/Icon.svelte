<script lang="ts" module>
  // every icon is a few thin, rounded strokes on a 24 by 24 grid, drawn
  // in the current text colour so icons follow light and dark mode.
  // they share one stroke weight and have no outline shapes around them,
  // so a row of them reads as one family.
  const paths = {
    back: "M14.5 5.5 8 12l6.5 6.5",
    open: "M3.5 8a2 2 0 0 1 2-2h3.6l2 2h7.4a2 2 0 0 1 2 2v7a2 2 0 0 1-2 2h-13a2 2 0 0 1-2-2Z",
    newWindow:
      "M4 6.5a2 2 0 0 1 2-2h12a2 2 0 0 1 2 2v11a2 2 0 0 1-2 2H6a2 2 0 0 1-2-2Z M4 9h16 M12 11.8v4.8 M9.6 14.2h4.8",
    zoomOut: "M10.5 17a6.5 6.5 0 1 0 0-13 6.5 6.5 0 0 0 0 13Z M15.2 15.2 20 20 M8 10.5h5",
    zoomIn: "M10.5 17a6.5 6.5 0 1 0 0-13 6.5 6.5 0 0 0 0 13Z M15.2 15.2 20 20 M8 10.5h5 M10.5 8v5",
    fitWidth: "M3.5 5v14 M20.5 5v14 M7 12h10 M9.5 9.5 7 12l2.5 2.5 M14.5 9.5 17 12l-2.5 2.5",
    nightMode: "M19.5 14.2A7.8 7.8 0 0 1 9.8 4.5a7.8 7.8 0 1 0 9.7 9.7Z",
    // a window with a panel on its trailing side: the usual sign for
    // "show the side panel"
    sidePanel:
      "M4 6.5a2 2 0 0 1 2-2h12a2 2 0 0 1 2 2v11a2 2 0 0 1-2 2H6a2 2 0 0 1-2-2Z M14.5 4.5v15 M16.6 8h1.3 M16.6 10.6h1.3",
    // the mirror image of sidePanel: a window with a panel on its leading
    // side, the usual sign for "show the sidebar"
    sidebar:
      "M4 6.5a2 2 0 0 1 2-2h12a2 2 0 0 1 2 2v11a2 2 0 0 1-2 2H6a2 2 0 0 1-2-2Z M9.5 4.5v15 M6.1 8h1.3 M6.1 10.6h1.3",
    // two pages side by side, like an open book
    twoPages:
      "M3.5 6.5A1.5 1.5 0 0 1 5 5h4.5A1.5 1.5 0 0 1 11 6.5v11A1.5 1.5 0 0 1 9.5 19H5a1.5 1.5 0 0 1-1.5-1.5Z M13 6.5A1.5 1.5 0 0 1 14.5 5H19a1.5 1.5 0 0 1 1.5 1.5v11A1.5 1.5 0 0 1 19 19h-4.5a1.5 1.5 0 0 1-1.5-1.5Z",
    // an eye: options for how things look
    view: "M2.5 12s3.5-6.5 9.5-6.5 9.5 6.5 9.5 6.5-3.5 6.5-9.5 6.5S2.5 12 2.5 12Z M12 14.8a2.8 2.8 0 1 0 0-5.6 2.8 2.8 0 0 0 0 5.6Z",
    // the folder of "open", with a plus: add a folder
    pinFolder:
      "M3.5 8a2 2 0 0 1 2-2h3.6l2 2h7.4a2 2 0 0 1 2 2v7a2 2 0 0 1-2 2h-13a2 2 0 0 1-2-2Z M12 10.8v5.4 M9.3 13.5h5.4",
    search: "M10.5 17a6.5 6.5 0 1 0 0-13 6.5 6.5 0 0 0 0 13Z M15.2 15.2 20 20",
    chevronUp: "M6.5 14.5 12 9l5.5 5.5",
    chevronDown: "M6.5 9.5 12 15l5.5-5.5",
    chevronRight: "M9.5 6.5 15 12l-5.5 5.5",
    // three sliders: the usual sign for settings and tools
    tools: "M4.5 7h8.5 M17 7h2.5 M15 5v4 M4.5 12h2.5 M11 12h8.5 M9 10v4 M4.5 17h10 M18.5 17h1 M16.5 15v4",
    arrowUp: "M12 18.5v-13 M6.5 11 12 5.5l5.5 5.5",
    arrowDown: "M12 5.5v13 M6.5 13 12 18.5l5.5-5.5",
    check: "M5.5 12.5l4 4 9-9",
    close: "M6.5 6.5l11 11 M17.5 6.5l-11 11",
    plus: "M12 5.5v13 M5.5 12h13",
    // two arrows passing each other: one type becoming another (Convert File…)
    convert: "M5 8.5h13 M15 5.5l3 3-3 3 M19 15.5H6 M9 12.5l-3 3 3 3",
  };

  export type IconName = keyof typeof paths;
</script>

<script lang="ts">
  interface Props {
    name: IconName;
    /** width and height in rem */
    size?: number;
  }

  let { name, size = 1.25 }: Props = $props();
</script>

<!-- icons never carry meaning on their own: the button around them has the label -->
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

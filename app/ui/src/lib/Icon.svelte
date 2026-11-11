<script lang="ts" module>
  // every icon is a few thin, rounded strokes on a 24 by 24 grid, drawn
  // in the current text colour so icons follow light and dark mode.
  const paths = {
    back: "M14.5 5.5 8 12l6.5 6.5",
    open: "M3.5 8a2 2 0 0 1 2-2h3.6l2 2h7.4a2 2 0 0 1 2 2v7a2 2 0 0 1-2 2h-13a2 2 0 0 1-2-2Z",
    newWindow: "M4 6.5a2 2 0 0 1 2-2h12a2 2 0 0 1 2 2v11a2 2 0 0 1-2 2H6a2 2 0 0 1-2-2Z M4 9h16 M12 11.8v4.8 M9.6 14.2h4.8",
    zoomOut: "M10.5 17a6.5 6.5 0 1 0 0-13 6.5 6.5 0 0 0 0 13Z M15.2 15.2 20 20 M8 10.5h5",
    zoomIn: "M10.5 17a6.5 6.5 0 1 0 0-13 6.5 6.5 0 0 0 0 13Z M15.2 15.2 20 20 M8 10.5h5 M10.5 8v5",
    fitWidth: "M3.5 5v14 M20.5 5v14 M7 12h10 M9.5 9.5 7 12l2.5 2.5 M14.5 9.5 17 12l-2.5 2.5",
    nightMode: "M19.5 14.2A7.8 7.8 0 0 1 9.8 4.5a7.8 7.8 0 1 0 9.7 9.7Z",
    help: "M12 21a9 9 0 1 0 0-18 9 9 0 0 0 0 18Z M9.6 9.4a2.5 2.5 0 1 1 3.6 2.3c-.8.5-1.2 1-1.2 2 M12 16.9v.1",
    search: "M10.5 17a6.5 6.5 0 1 0 0-13 6.5 6.5 0 0 0 0 13Z M15.2 15.2 20 20",
    close: "M6.5 6.5l11 11 M17.5 6.5l-11 11",
    plus: "M12 5.5v13 M5.5 12h13",
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
  stroke-width="1.6"
  stroke-linecap="round"
  stroke-linejoin="round"
  aria-hidden="true"
>
  <path d={paths[name]} />
</svg>

<script lang="ts">
  // the Edit tools' window-wide parts: while Edit Text and Images (or Add
  // Image) is on, a picture pasted with Ctrl V goes onto the page in view,
  // and a png or jpeg dropped from the system goes where it lands. drops
  // anywhere else (the thumbnails, the rest of the window) go on to
  // whoever took them before. the Edit tab's sheets show here too: the
  // question before changing a signed file, what to do when re-wrapped
  // text doesn't fit, and Page Setup.
  import { isTextField } from "../commands";
  import { fileDropTarget, setFileDropTarget } from "../fileDrop";
  import type { PagePoint } from "../pageTools/pagePoints";
  import ConsentSheet from "./ConsentSheet.svelte";
  import type { EditSession } from "./editSession.svelte";
  import OverflowSheet from "./OverflowSheet.svelte";
  import PageSetupSheet from "./PageSetupSheet.svelte";

  interface Props {
    session: EditSession;
  }

  let { session }: Props = $props();

  const listening = $derived(session.tool === "edit" || session.tool === "addImage");

  /** the page under a point of the window, from the overlay laid over each page. */
  function pageAt(x: number, y: number): PagePoint | null {
    for (const element of document.elementsFromPoint(x, y)) {
      const page = (element as HTMLElement).closest?.("[data-edit-page]") as HTMLElement | null;
      if (!page) continue;
      const box = page.getBoundingClientRect();
      const scale = Number(page.dataset.scale) || 1;
      return { page: Number(page.dataset.editPage), x: (x - box.left) / scale, y: (y - box.top) / scale };
    }
    return null;
  }

  const isPicture = (path: string) => /\.(png|jpe?g)$/i.test(path);

  $effect(() => {
    if (!listening) return;
    const before = fileDropTarget();
    return setFileDropTarget({
      over: (x, y) => (pageAt(x, y) !== null ? true : (before?.over(x, y) ?? false)),
      drop: (paths, point) => {
        const at = point ? pageAt(point.x, point.y) : null;
        const picture = paths.find(isPicture);
        if (at && picture) {
          session.pictures.drop(picture, at);
          return true;
        }
        return before?.drop(paths, point) ?? false;
      },
      leave: () => before?.leave(),
    });
  });

  $effect(() => {
    if (!listening) return;
    const onPaste = (event: ClipboardEvent) => {
      if (isTextField(document.activeElement)) return;
      const file = [...(event.clipboardData?.files ?? [])].find((each) => each.type === "image/png" || each.type === "image/jpeg");
      if (!file) return;
      event.preventDefault();
      pasted(file);
    };
    document.addEventListener("paste", onPaste);
    return () => document.removeEventListener("paste", onPaste);
  });

  async function pasted(file: File) {
    const bytes = new Uint8Array(await file.arrayBuffer());
    let aspect = 1;
    try {
      const bitmap = await createImageBitmap(file);
      aspect = bitmap.width / Math.max(bitmap.height, 1);
      bitmap.close();
    } catch {
      // the shell reads the picture anyway; a square place is a fine start
    }
    session.pictures.paste(bytes, aspect);
  }
</script>

{#if session.consent.asking}
  <ConsentSheet question={session.consent.asking} />
{:else if session.flow.pending}
  <OverflowSheet flow={session.flow} />
{:else if session.pageSetup.open}
  <PageSetupSheet setup={session.pageSetup} />
{/if}

<script lang="ts">
  // the Arrange tab: the selected object's exact place, size and angle,
  // stacking (front, back, one step), grouping, a fill from the theme's
  // colours, and for a table its rows and columns. every change is one
  // operation and one undo step; what the file keeps for the object that
  // isn't drawn here is said in its own sentence.
  import { tools } from "../../tools";
  import type { DeckSession } from "../deckSession.svelte";
  import { resizeTo, rotateTo, themeColor, type Operation, type ZOrder } from "../operations";

  interface Props {
    session: DeckSession;
    onAnnounce: (words: string) => void;
  }

  let { session, onAnnounce }: Props = $props();

  const UNIT = navigator.language === "en-US" ? "in" : "cm";
  const POINTS_PER_UNIT = UNIT === "in" ? 72 : 72 / 2.54;
  /** the theme slots a fill can follow, as keynote's and powerpoint's swatches start */
  const FILL_SLOTS = ["accent1", "accent2", "accent3", "accent4", "accent5", "accent6", "dk2", "lt2"];

  const slide = $derived(session.slide);
  const selected = $derived((session.view?.objects ?? []).filter((object) => session.selection.includes(object.id)));
  const only = $derived(selected.length === 1 ? selected[0] : null);
  const colors = $derived(session.deck.theme?.colors ?? []);

  const show = (points: number) => Number((points / POINTS_PER_UNIT).toFixed(2));

  function apply(operation: Operation, words: string) {
    void session.apply(operation, [...session.selection]).then((done) => done && onAnnounce(words));
  }

  function setBox(field: "x" | "y" | "width" | "height", value: number) {
    if (!only || !slide || !Number.isFinite(value)) return;
    const box = { x: only.x, y: only.y, width: only.width, height: only.height, [field]: value * POINTS_PER_UNIT };
    if (box.width <= 0 || box.height <= 0) return;
    apply(resizeTo(slide.id, only.id, box), "Size and place changed.");
  }

  function setAngle(value: number) {
    if (!only || !slide || !Number.isFinite(value)) return;
    apply(rotateTo(slide.id, only.id, value), `Turned to ${value}°.`);
  }

  function restack(to: ZOrder) {
    if (!slide) return;
    for (const object of selected) apply({ op: "reorder", slide_id: slide.id, object_id: object.id, to }, "Stacking changed.");
  }

  function fill(slot: string | null) {
    if (!only || !slide) return;
    const value = slot ? { type: "solid" as const, color: themeColor(slot) } : { type: "none" as const };
    apply({ op: "setFill", slide_id: slide.id, object_id: only.id, fill: value }, slot ? "Fill changed." : "Fill removed.");
  }

  function table(op: "insertRow" | "deleteRow" | "insertColumn" | "deleteColumn", after: boolean) {
    if (!only || !slide) return;
    const cell = session.tableCell?.object === only.id ? session.tableCell : { row: 0, column: 0 };
    const index = op.includes("Row") ? cell.row : cell.column;
    apply({ op, slide_id: slide.id, object_id: only.id, at: op.startsWith("insert") && after ? index + 1 : index }, "Table changed.");
  }

  const numberInput = (event: Event) => Number((event.currentTarget as HTMLInputElement).value);
</script>

{#if selected.length === 0}
  <section>
    <p class="hint">Select an object on the slide to place, size, turn or stack it. Shift-click selects several.</p>
  </section>
{:else}
  {#if only}
    <section>
      <h3>{only.name || "Object"}</h3>
      <div class="setting-row">
        {#each [["x", "Left"], ["y", "Top"], ["width", "Width"], ["height", "Height"]] as [field, label] (field)}
          <label class="setting">
            <span class="caption">{label} ({UNIT})</span>
            <input class="field" type="number" step="0.1" value={show(only[field as "x"])} onchange={(event) => setBox(field as "x", numberInput(event))} />
          </label>
        {/each}
        <label class="setting">
          <span class="caption">Angle (°)</span>
          <input class="field" type="number" step="15" value={Math.round(only.rotation * 10) / 10} onchange={(event) => setAngle(numberInput(event))} />
        </label>
      </div>
      {#if only.kept}<p class="hint">{only.kept}</p>{/if}
    </section>
  {/if}

  <section>
    <h3>{tools.sArrange.label}</h3>
    <div class="buttons">
      <button class="button" onclick={() => restack("front")}>Bring to Front</button>
      <button class="button" onclick={() => restack("back")}>Send to Back</button>
      <button class="button" onclick={() => restack("forward")}>Bring Forward</button>
      <button class="button" onclick={() => restack("backward")}>Send Backward</button>
      {#if selected.length > 1 && slide}
        <button class="button" title={tools.sGroup.tooltip} onclick={() => apply({ op: "group", slide_id: slide!.id, object_ids: selected.map((each) => each.id) }, "Grouped.")}>Group</button>
      {/if}
      {#if only?.kind === "group" && slide}
        <button class="button" onclick={() => apply({ op: "ungroup", slide_id: slide!.id, group_id: only!.id }, "Ungrouped.")}>Ungroup</button>
      {/if}
      <button class="button" onclick={() => void session.duplicate()}>Duplicate</button>
    </div>
    <p class="hint">{tools.sArrange.meaning}</p>
  </section>

  {#if only && (only.kind === "shape" || only.kind === "textBox" || only.kind === "placeholder")}
    <section>
      <h3>Fill</h3>
      <div class="swatches" role="group" aria-label="Theme colours">
        {#each FILL_SLOTS as slot (slot)}
          {@const hex = colors.find((color) => color.slot === slot)?.hex ?? "#888888"}
          <button class="swatch" style:background={hex} title="Theme colour {slot}: follows the theme" aria-label="Fill with theme colour {slot}" onclick={() => fill(slot)}></button>
        {/each}
        <button class="button" onclick={() => fill(null)}>No Fill</button>
      </div>
      <p class="hint">Theme colours change when the theme does (see the Theme tab).</p>
    </section>
  {/if}

  {#if only?.kind === "table"}
    <section>
      <h3>{tools.sTableRows.label}</h3>
      <p class="hint">Works on the cell you last typed in{session.tableCell?.object === only.id ? `, row ${session.tableCell.row + 1}, column ${session.tableCell.column + 1}` : "; double-click a cell to choose one"}.</p>
      <div class="buttons">
        <button class="button" onclick={() => table("insertRow", false)}>Row Above</button>
        <button class="button" onclick={() => table("insertRow", true)}>Row Below</button>
        <button class="button" onclick={() => table("deleteRow", false)}>Delete Row</button>
        <button class="button" onclick={() => table("insertColumn", false)}>Column Left</button>
        <button class="button" onclick={() => table("insertColumn", true)}>Column Right</button>
        <button class="button" onclick={() => table("deleteColumn", false)}>Delete Column</button>
      </div>
      <p class="hint">{tools.sTableRows.fileEffect}</p>
    </section>
  {/if}
{/if}

<style>
  .buttons {
    display: flex;
    flex-wrap: wrap;
    gap: 0.375rem;
  }

  .swatches {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 0.375rem;
  }

  .swatch {
    width: 1.5rem;
    height: 1.5rem;
    padding: 0;
    border: 0;
    border-radius: 50%;
    box-shadow: 0 0 0 0.5px var(--separator);
  }
</style>

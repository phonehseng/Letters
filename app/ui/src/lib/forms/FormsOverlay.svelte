<script lang="ts">
  // the Fill & Sign controls that sit on one page: the form's fields as
  // soft tinted boxes to click or Tab into (or, in Edit Fields, outlines
  // to select, move and resize), Find Fields' proposals as dashed outlines
  // with their labels, and the place drawn for a digital signature.
  //
  // the viewer puts one of these over every page on screen. everything
  // here carries data-page-ui, so the page tool under it never hears the
  // presses; with another page tool in use (`quiet`) the boxes step aside.
  import ObjectFrame from "../pageTools/ObjectFrame.svelte";
  import { HANDLES } from "../pageTools/objectFrame";
  import type { PageFrame, Rect } from "../pageTools/pagePoints";
  import FieldBox from "./FieldBox.svelte";
  import type { FillSignSession } from "./fillSignSession.svelte";
  import { KIND_LABELS, proposalLabel } from "./proposals";
  import { fieldLabel, readingOrder } from "./tabOrder";

  interface Props {
    session: FillSignSession;
    frame: PageFrame;
    /** another page tool is in use: the boxes let presses through */
    quiet: boolean;
    onFocusDocument: () => void;
  }

  let { session, frame, quiet, onFocusDocument }: Props = $props();

  // every place of every field on this page, in the order Tab visits them
  const places = $derived(
    session.fields
      .flatMap((field) =>
        field.widgets.map((widget, index) => ({ field, index, page: widget.page ?? -1, rect: widget.rect })),
      )
      .filter((place) => place.page === frame.index)
      .sort(readingOrder),
  );
  const proposals = $derived(session.proposals.filter((proposal) => proposal.page === frame.index));
  const selectedProposal = $derived(proposals.find((proposal) => proposal.id === session.selectedProposal) ?? null);
  const selected = $derived.by(() => {
    const chosen = session.selectedField;
    if (!chosen || session.mode !== "edit") return null;
    const field = session.field(chosen.field);
    const widget = field?.widgets[chosen.widget];
    return field && widget && widget.page === frame.index ? { field, widget: chosen.widget, rect: widget.rect } : null;
  });
  const signArea = $derived(session.signArea?.page === frame.index ? session.signArea.rect : null);
  const passive = $derived(quiet || session.tool !== "none");

  function box(rect: Rect) {
    return `left:${rect.x * frame.scale}px;top:${rect.y * frame.scale}px;width:${rect.width * frame.scale}px;height:${rect.height * frame.scale}px`;
  }

  function previewFor(field: string, widget: number, rect: Rect): Rect {
    const preview = session.framePreview;
    return preview && preview.field === field && preview.widget === widget ? preview.rect : rect;
  }
</script>

{#if session.showFields && places.length > 0}
  <div
    class="layer"
    class:passive
    data-page-ui={passive ? undefined : ""}
    style:left="{frame.left}px"
    style:top="{frame.top}px"
    style:width="{frame.width}px"
    style:height="{frame.height}px"
  >
    {#each places as place (`${place.field.name}#${place.index}`)}
      {#if session.mode === "fill"}
        <FieldBox {session} field={place.field} widget={place.index} {frame} {onFocusDocument} />
      {:else if !(selected && selected.field.name === place.field.name && selected.widget === place.index)}
        <button
          class="outline"
          style={box(previewFor(place.field.name, place.index, place.rect))}
          title={fieldLabel(place.field)}
          aria-label={`Field ${fieldLabel(place.field)}. Press to select it.`}
          onpointerdown={(event) => {
            event.preventDefault();
            session.selectField(place.field.name, place.index);
          }}
          onclick={() => session.selectField(place.field.name, place.index)}
        >
          <span class="name">{place.field.name}</span>
        </button>
      {/if}
    {/each}
  </div>
{/if}

{#if selected}
  {#key `${selected.field.name}#${selected.widget}`}
    <ObjectFrame
      {frame}
      rect={previewFor(selected.field.name, selected.widget, selected.rect)}
      label={`Field ${fieldLabel(selected.field)} on page ${frame.index + 1}. The arrow keys move it, with Shift for 10 points; Alt with the arrows changes its size. Delete deletes it. Escape lets go.`}
      movable={true}
      handles={selected.field.kind === "radioGroup" || selected.field.kind === "checkbox" ? HANDLES.filter((handle) => handle.length === 2) : HANDLES}
      keepAspect={selected.field.kind === "checkbox" || selected.field.kind === "radioGroup"}
      focusOnShow={true}
      onPreview={(rect) => (session.framePreview = { field: selected.field.name, widget: selected.widget, rect })}
      onCommit={(rect) => session.moveField(selected.field.name, selected.widget, rect)}
      onDelete={() => {
        session.deleteField(selected.field.name);
        onFocusDocument();
      }}
      onEscape={() => {
        session.selectedField = null;
        onFocusDocument();
      }}
    />
  {/key}
{/if}

{#if proposals.length > 0}
  <div
    class="layer"
    class:passive
    data-page-ui={passive ? undefined : ""}
    style:left="{frame.left}px"
    style:top="{frame.top}px"
    style:width="{frame.width}px"
    style:height="{frame.height}px"
  >
    {#each proposals as proposal (proposal.id)}
      {#if proposal.id !== selectedProposal?.id}
        <button
          class="proposal {proposal.status}"
          style={box(proposal.rect)}
          aria-label={`Suggested ${KIND_LABELS[proposal.kind].toLowerCase()} field, ${proposalLabel(proposal)}, ${proposal.status === "pending" ? "not decided" : proposal.status}. Press to adjust it.`}
          onpointerdown={(event) => {
            event.preventDefault();
            session.selectProposal(proposal.id);
          }}
          onclick={() => session.selectProposal(proposal.id)}
        ></button>
        <!-- small boxes side by side would bury each other's labels; the tab lists them all -->
        {#if proposal.rect.width * frame.scale >= 60}
          <span class="chip {proposal.status}" style:left="{proposal.rect.x * frame.scale}px" style:top="{proposal.rect.y * frame.scale}px">
            {proposalLabel(proposal)} · {KIND_LABELS[proposal.kind]}
          </span>
        {/if}
      {/if}
    {/each}
  </div>
{/if}

{#if selectedProposal}
  {#key selectedProposal.id}
    <ObjectFrame
      {frame}
      rect={selectedProposal.rect}
      label={`Suggested field ${proposalLabel(selectedProposal)}. The arrow keys move it, Alt with the arrows changes its size. Enter accepts it, Delete rejects it, Escape lets go.`}
      movable={true}
      handles={HANDLES}
      focusOnShow={true}
      keyCommitDelay={0}
      onCommit={(rect) => session.moveProposal(selectedProposal.id, rect)}
      onDelete={() => {
        session.setProposalStatus(selectedProposal.id, "rejected");
        session.selectedProposal = null;
        onFocusDocument();
      }}
      onOpen={() => session.setProposalStatus(selectedProposal.id, "accepted")}
      onEscape={() => {
        session.selectedProposal = null;
        onFocusDocument();
      }}
    />
  {/key}
{/if}

{#if signArea}
  <div class="layer passive" style:left="{frame.left}px" style:top="{frame.top}px" style:width="{frame.width}px" style:height="{frame.height}px">
    <div class="sign-area" style={box(signArea)}><span>Signature goes here</span></div>
  </div>
{/if}

<style>
  .layer {
    position: absolute;
    z-index: 1;
    pointer-events: none;
  }

  .outline,
  .proposal {
    position: absolute;
    margin: 0;
    padding: 0;
    pointer-events: auto;
    background: transparent;
  }

  .layer.passive .outline,
  .layer.passive .proposal {
    pointer-events: none;
  }

  /* editing fields: plain outlines with the field's name, so the form's structure shows */
  .outline {
    border: 1px solid rgb(52 120 246 / 0.7);
    border-radius: 2px;
    background: rgb(52 120 246 / 0.06);
    overflow: hidden;
  }

  .outline .name {
    position: absolute;
    top: 0;
    left: 2px;
    color: rgb(30 80 190);
    font: var(--footnote);
    white-space: nowrap;
  }

  /* proposals: dashed while undecided, solid once accepted, faint once rejected */
  .proposal {
    border: 1.5px dashed rgb(52 120 246 / 0.85);
    border-radius: 2px;
  }

  .proposal.accepted {
    border: 1.5px solid rgb(40 160 90);
    background: rgb(40 160 90 / 0.08);
  }

  .proposal.rejected {
    border: 1px dashed rgb(0 0 0 / 0.25);
  }

  .chip {
    position: absolute;
    transform: translateY(-100%);
    padding: 0 0.25rem;
    border-radius: 0.25rem 0.25rem 0 0;
    background: rgb(52 120 246 / 0.85);
    color: #ffffff;
    font: var(--footnote);
    white-space: nowrap;
    pointer-events: none;
  }

  .chip.accepted {
    background: rgb(40 150 85);
  }

  .chip.rejected {
    background: rgb(0 0 0 / 0.3);
  }

  .sign-area {
    position: absolute;
    display: grid;
    place-items: center;
    border: 1.5px dashed rgb(52 120 246 / 0.85);
    background: rgb(52 120 246 / 0.06);
    color: rgb(30 80 190);
    font: var(--footnote);
  }
</style>

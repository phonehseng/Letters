<script lang="ts">
  // the Annotate controls that sit on one page: the frame around the
  // selected annotation, a note's pop-up and a text box being typed.
  // the viewer puts one of these over every page on screen.
  import ObjectFrame from "../pageTools/ObjectFrame.svelte";
  import { handlesFor } from "../pageTools/objectFrame";
  import type { PageFrame } from "../pageTools/pagePoints";
  import { frameBehaviour, kindLabel } from "./annotations";
  import type { AnnotateSession } from "./annotateSession.svelte";
  import NotePopover from "./NotePopover.svelte";
  import TextBoxEditor from "./TextBoxEditor.svelte";

  interface Props {
    session: AnnotateSession;
    frame: PageFrame;
    /** gives the keyboard back to the document, after Escape */
    onFocusDocument: () => void;
  }

  let { session, frame, onFocusDocument }: Props = $props();

  const selected = $derived(session.selected?.page === frame.index ? session.selected : null);
  const behaviour = $derived(selected ? frameBehaviour(selected) : null);
  const editingHere = $derived(session.editing?.page === frame.index ? session.editing : null);
  const noteHere = $derived(session.note?.page === frame.index ? session.note : null);

  function frameLabel(): string {
    if (!selected || !behaviour) return "";
    const what = `${kindLabel(selected)} on page ${frame.index + 1}.`;
    if (!selected.editable) return `${what} Letters can’t change it, but Delete deletes it. Escape lets go.`;
    const moves = behaviour.movable ? " The arrow keys move it, with Shift for 10 points." : "";
    const resizes = behaviour.resizable ? " Alt with the arrows changes its size." : "";
    const opens = selected.details.type === "textBox" ? " Enter edits its text." : selected.kind === "note" ? " Enter opens it." : "";
    return `${what}${moves}${resizes}${opens} Delete deletes it. Escape lets go.`;
  }

  function open() {
    if (!selected) return;
    if (selected.details.type === "textBox") session.editTextBox(selected);
    else if (selected.kind === "note") session.openNote(selected);
  }
</script>

{#if selected && behaviour && editingHere?.id !== selected.id}
  {#key selected.id}
    <ObjectFrame
      {frame}
      rect={session.framePreview ?? selected.rect}
      label={frameLabel()}
      movable={behaviour.movable}
      handles={handlesFor(behaviour.resizable, behaviour.keepAspect)}
      keepAspect={behaviour.keepAspect}
      focusOnShow={!noteHere}
      onPreview={(rect) => (session.framePreview = rect)}
      onCommit={(rect) => session.moveSelected(rect)}
      onDelete={() => {
        session.deleteSelected();
        onFocusDocument();
      }}
      onEscape={() => {
        session.deselect();
        onFocusDocument();
      }}
      onOpen={open}
    />
  {/key}
{/if}

{#if editingHere}
  {#key `${editingHere.id}-${editingHere.rect.x}-${editingHere.rect.y}`}
    <TextBoxEditor {session} {frame} />
  {/key}
{/if}

{#if noteHere}
  <NotePopover {session} {frame} />
{/if}

// the faces the slide is drawn with, handed to the web view, so the typing
// field shows the very letters the slide will have (a font such as aptos
// may live only in office's own folder, where the web view can't find it).
// each face is loaded once per deck under a name of its own and kept.

import { invoke } from "@tauri-apps/api/core";

const loaded = new Map<string, Promise<string | null>>();
let next = 1;

/** the css family name that draws `family` as the engine does, or null to use the family's own name. */
export function slideFace(deckId: number, family: string, bold: boolean, italic: boolean): Promise<string | null> {
  const key = `${deckId}|${family}|${bold}|${italic}`;
  let face = loaded.get(key);
  if (!face) {
    face = load(deckId, family, bold, italic).catch(() => null);
    loaded.set(key, face);
  }
  return face;
}

async function load(deckId: number, family: string, bold: boolean, italic: boolean): Promise<string | null> {
  const bytes = await invoke<ArrayBuffer>("slides_font_face", { id: deckId, family, bold, italic });
  if (!bytes || bytes.byteLength === 0) return null;
  const name = `Letters Slide Face ${next++}`;
  const face = new FontFace(name, bytes, { weight: bold ? "700" : "400", style: italic ? "italic" : "normal" });
  await face.load();
  document.fonts.add(face);
  return name;
}

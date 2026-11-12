// the font library's public face for the rest of the ui.
//
// the font picker, for any screen that sets a font:
//
//   import { FontPicker } from "../fonts";
//   <FontPicker value={font} documentFonts={used} onChange={(family) => setFont(family)} />
//
//   value          the chosen family's name ("" for none)
//   onChange       called with the family the person chose; an added or
//                  downloaded font is already loaded into the window then
//   documentFonts  fonts the open document uses (listed under In This Document)
//   label          read out to screen readers (default "Font")
//   disabled       greys the picker out
//
// the Font Library itself:
//
//   openFontLibrary({ documentFonts, onReplaceFont, family, view })
//   setDocumentFonts({ documentFonts, onReplaceFont })  // what Ctrl T opens with
//
// fonts that aren't installed (added, bundled, downloaded) are drawn by
// the web view once given to it: useFontInDocument(name) does that for a
// family's every style, and cssFamily(name) is its font-family value.

export { default as FontPicker } from "./FontPicker.svelte";
export { openFontLibrary, closeFontLibrary, setDocumentFonts, isFontLibraryOpen } from "./openFontLibrary";
export { useFontInDocument, cssFamily } from "./fontFaces";
export { fontsApi } from "./fontsApi";
export type { FamilySummary, FamilyDetails, MissingFont, Substitution } from "./fontsApi";

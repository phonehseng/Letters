// small reads from the page that several components need.

/**
 * how many css pixels one rem is right now. it follows the system text
 * size, so layouts measured in pixels can still be planned in rem.
 */
export function remInPixels(): number {
  return parseFloat(getComputedStyle(document.documentElement).fontSize) || 16;
}

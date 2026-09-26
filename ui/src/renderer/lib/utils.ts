export { cn } from "cn"

/**
 * What a dialog darkens the window with.
 *
 * Darker and blurrier than the shadcn default, because of what is underneath it: this window is
 * drawn over a native sidebar blur, and a ten-percent scrim over that reads as a smudge rather
 * than as something having come to the front. Held here rather than in `ui/dialog.tsx` so the
 * shadcn CLI can regenerate that file without taking this with it.
 *
 * The theme picker passes its own on purpose — see the note there.
 */
export const SCRIM = 'scrim bg-black/40 supports-backdrop-filter:backdrop-blur-[3px]'

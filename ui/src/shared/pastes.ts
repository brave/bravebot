/**
 * What the window is told about a picture a person pasted.
 *
 * The bytes never cross to the page. The main process reads the clipboard itself, keeps the picture
 * against an opaque id bound to the session, and the page is told the id and a small drawing of it.
 */

/** The most a pasted picture may weigh: the terminal's `MAX_IMAGE_BYTES` and the bridge's cap. */
export const MAX_PASTED_BYTES = 10 * 1024 * 1024

/** A picture a person pasted, as the window knows it. */
export interface PastedPicture {
  id: string
  /** A small drawing of the picture, as a `data:` URL the main process drew. */
  thumbnail?: string
}

/**
 * What one paste came to. `too-large` carries the picture's size and the cap, both in bytes, so the
 * window can say both as the terminal's note does.
 */
export type PasteOutcome =
  | { kind: 'staged'; picture: PastedPicture }
  | { kind: 'too-large'; size: number; limit: number }

/** One paste into a session's composer, as the preload tells the page. */
export interface Paste {
  session: string
  outcome: PasteOutcome
}

/** What the window says about a picture too large to paste, with both sizes as the terminal's note has them. */
export function tooLargeNote(size: number, limit: number): string {
  return `That picture is ${inMegabytes(size)}, and a paste carries at most ${inMegabytes(limit)}.`
}

/** `12.3 MB`, the terminal's `in_megabytes`. */
function inMegabytes(bytes: number): string {
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`
}

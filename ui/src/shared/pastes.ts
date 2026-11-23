/**
 * What the window is told about a picture a person pasted.
 *
 * The bytes never cross to the page. The main process reads the clipboard itself, keeps the picture
 * against an opaque id bound to the session, and the page is told the id and a small drawing of it.
 */

/** A picture a person pasted, as the window knows it. */
export interface PastedPicture {
  id: string
  /** The word the marker uses, the terminal's, from the bridge's `pastes.check`. */
  noun: string
  /** A small drawing of the picture, as a `data:` URL the main process drew. */
  thumbnail?: string
}

/**
 * What one paste came to. `too-large` carries the bridge's note, the terminal's, which says the
 * picture's size and the cap, for the window to show as it stands. `text` is pasted text with the
 * characters that draw as nothing removed by the bridge's `pastes.clean` (PASTE-10): how many were
 * removed, and the terminal's note saying so when there were any. `unclean` is text the bridge did
 * not clean, which is not pasted.
 */
export type PasteOutcome =
  | { kind: 'staged'; picture: PastedPicture }
  | { kind: 'too-large'; note: string }
  | { kind: 'text'; text: string; removed: number; note?: string }
  | { kind: 'unclean' }

/** One paste into a session's composer, as the preload tells the page. */
export interface Paste {
  session: string
  outcome: PasteOutcome
}

/**
 * The longest text a folded paste may hold, the most the preferences file keeps of one. A longer
 * paste is not folded and lands whole, so every folded paste a draft names survives a reopen.
 */
export const FOLDED_TEXT_MAX = 200000

/**
 * A long text a person pasted, folded behind a marker in the composer, as the terminal's
 * `PastedText` (PASTE-1). The words stay in the page: text is presentation, so no grant is minted.
 */
export interface PastedText {
  /** The text standing for it in the draft, as `[Pasted text #1 +12 lines]`. */
  marker: string
  /** What was pasted, with its line endings already normalised. */
  text: string
  /** The number the marker carries, from the counter drops and pictures share. */
  number: number
  /** How many lines the text is, as `linesIn` counts them. */
  lines: number
}

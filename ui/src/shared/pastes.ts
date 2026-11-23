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
 * picture's size and the cap, for the window to show as it stands.
 */
export type PasteOutcome =
  | { kind: 'staged'; picture: PastedPicture }
  | { kind: 'too-large'; note: string }

/** One paste into a session's composer, as the preload tells the page. */
export interface Paste {
  session: string
  outcome: PasteOutcome
}

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

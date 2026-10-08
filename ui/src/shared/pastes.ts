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

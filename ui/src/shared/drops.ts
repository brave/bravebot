/**
 * What a file dropped on the window is, and what the window is told about one.
 *
 * What kind of file a name is, the word its marker uses, and the note for one too large to carry
 * are the bridge's answer to `drops.classify`, from the rules the terminal stages a drop by
 * (`bravebot_filetype::by_name`) and the agent's attachment cap. The window holds no copy of them.
 */

export type DropKind = 'image' | 'pdf' | 'text'

/**
 * A file somebody dropped, as the window knows it: an opaque grant id and a name to show.
 *
 * Never the path. The main process holds that against the id, so nothing the page can say turns
 * into a file the agent reads.
 */
export interface DroppedFile {
  id: string
  name: string
  kind: DropKind
  /**
   * The word the marker uses: `Image`, `PDF` or `File`. Not translated, as in the terminal: the
   * marker is sent to the model as it stands, and the model counts from it to the picture that
   * answers it.
   */
  noun: string
  /** A small picture of a dropped image, as a `data:` URL the preload drew. */
  thumbnail?: string
}

/**
 * What became of one path in a drop, in the order the files were dropped.
 *
 * `named` is a type nothing here takes, whose path is written into the line as text and attaches
 * nothing (DROP-4). `skipped` is a folder (DROP-5), a file too large to carry, or one that could not
 * be read, which the window reports and leaves out. A file too large carries the bridge's note,
 * which says its size and the cap, for the window to show as it stands.
 */
export type DropOutcome =
  | { kind: 'staged'; file: DroppedFile }
  | { kind: 'named'; text: string }
  | { kind: 'skipped'; name: string; why: 'folder' | 'unreadable' }
  | { kind: 'skipped'; name: string; why: 'too-large'; note: string }

/** What one drop onto a session's conversation came to, as the preload tells the page. */
export interface Drop {
  session: string
  outcomes: DropOutcome[]
}

/**
 * Naming a project file with `@` in the composer (NAME-9).
 *
 * The rules live in the `bravebot-mentions` crate, which the terminal calls directly and the
 * window reaches through the bridge's `mentions.offer` and `mentions.named`. What is left here is
 * the shape of the answer and the one edit the composer makes to its own draft.
 */

/** One thing in the project a mention could name. */
export interface MentionEntry {
  /** Project-relative, with a trailing slash for a directory so it reads as one. */
  path: string
  /** The path as it is written in a line, a space behind a backslash (NAME-8). From the bridge. */
  written: string
  directory: boolean
}

/** The bridge's answer to `mentions.offer` for one line and one cursor row. */
export interface MentionOffer {
  /** What follows the `@` of the last word while it is being typed, else `null`. */
  typed: string | null
  /** The last word of the line, which a completion replaces. From the bridge, as the terminal's is. */
  word: string | null
  entries: MentionEntry[]
  /** NAME-7: whether Enter on the cursor's row completes the name rather than sending the line. */
  completes: boolean
}

/**
 * The draft with its last word replaced by the chosen entry. The rest of the sentence is kept,
 * a file gets a trailing space so the list closes, and a directory does not, so typing continues
 * into it. The word and the way the name is written are the bridge's, so a name holding a space
 * is completed to the escaped text the terminal writes and not to a different path.
 */
export function acceptMention(line: string, entry: MentionEntry, word: string | null): string {
  if (word === null || !word.startsWith('@') || !line.endsWith(word)) return line
  return `${line.slice(0, line.length - word.length)}@${entry.written}${entry.directory ? '' : ' '}`
}

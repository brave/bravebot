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
  /** `path` drawn so a character that reads as whitespace or as nothing is visible (NAME-4). From the bridge. */
  shownPath: string
  /** The path as it is written in a line: each whitespace character and backslash behind a backslash (NAME-8). From the bridge. */
  written: string
  directory: boolean
  /** Which kind of thing the row names. A `definition` row is addressed for one turn and never written as an `@` name (ADDRESS-2). From the bridge. */
  kind: 'file' | 'directory' | 'session' | 'definition'
  /** A definition row's description, else `null`. From the bridge. */
  description: string | null
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

/** The bridge's answer to `mentions.offer`, or nothing offered when it is out of shape. Each row is kept as the bridge sent it, its `kind` and `description` with it. */
export function offerOf(answer: Partial<MentionOffer> | undefined): MentionOffer {
  return typeof answer?.typed === 'string' && Array.isArray(answer.entries)
    ? { typed: answer.typed, word: typeof answer.word === 'string' ? answer.word : null, entries: answer.entries, completes: answer.completes === true }
    : { typed: null, word: null, entries: [], completes: false }
}

/** The definition a row addresses when it is picked, or `null` for a row that is written as an `@` name (ADDRESS-2). */
export function definitionOf(entry: MentionEntry): { name: string; description: string | null } | null {
  return entry.kind === 'definition' ? { name: entry.path, description: entry.description ?? null } : null
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

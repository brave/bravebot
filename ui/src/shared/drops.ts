/**
 * What a file dropped on the window is, and what the window is told about one.
 *
 * The tables are the terminal's (`crates/tui/src/dropped.rs`) and the agent's
 * (`crates/agent/src/workspace.rs` `ATTACHABLE`), so a file dropped on either front end is the same
 * kind of file. `scripts/drops.test.mjs` reads both Rust tables and fails when these drift from
 * them.
 */

/** Extensions carried to the model as bytes, with the media type the agent names them by. */
export const ATTACHABLE: readonly (readonly [extension: string, media: string])[] = [
  ['png', 'image/png'],
  ['jpg', 'image/jpeg'],
  ['jpeg', 'image/jpeg'],
  ['gif', 'image/gif'],
  ['webp', 'image/webp'],
  ['pdf', 'application/pdf'],
]

/** Extensions read into the turn as text. */
export const TEXTUAL: readonly string[] = [
  'txt', 'md', 'markdown', 'rst', 'adoc', 'org', 'rs', 'py', 'js', 'jsx', 'mjs', 'cjs', 'ts', 'tsx',
  'vue', 'svelte', 'json', 'jsonc', 'yaml', 'yml', 'toml', 'ini', 'cfg', 'conf', 'properties', 'env',
  'html', 'htm', 'xml', 'svg', 'css', 'scss', 'sass', 'less', 'sh', 'bash', 'zsh', 'fish', 'ps1',
  'bat', 'c', 'h', 'cc', 'cpp', 'cxx', 'hpp', 'hh', 'java', 'kt', 'kts', 'go', 'rb', 'php', 'swift',
  'm', 'mm', 'cs', 'scala', 'clj', 'cljs', 'ex', 'exs', 'erl', 'hs', 'lua', 'pl', 'pm', 'r', 'jl',
  'dart', 'zig', 'nim', 'sql', 'graphql', 'proto', 'csv', 'tsv', 'log', 'diff', 'patch', 'lock',
  'gradle', 'tf', 'tfvars', 'dockerfile', 'mk', 'cmake',
]

/** Names that are text without an extension to say so. */
export const TEXTUAL_NAMES: readonly string[] = [
  'makefile', 'dockerfile', 'readme', 'license', 'licence', 'changelog', 'authors', 'notice',
  'gemfile', 'rakefile', 'procfile', 'justfile', 'vagrantfile', 'brewfile',
]

/** Name prefixes that are text whatever follows them: `.gitignore`, `.gitattributes`. */
export const TEXTUAL_PREFIXES: readonly string[] = ['gitignore', 'gitattr']

export type DropKind = 'image' | 'pdf' | 'text'

/**
 * The word a marker uses: `[Image #1]`, `[PDF #2]`, `[File #3]`.
 *
 * Not translated, as in the terminal: the marker is sent to the model as it stands, and the model
 * counts from it to the picture that answers it.
 */
export const NOUN: Record<DropKind, string> = { image: 'Image', pdf: 'PDF', text: 'File' }

/** The most the agent carries as bytes (`MAX_ATTACHMENT_BYTES`). */
export const MAX_ATTACHMENT_BYTES = 8 * 1024 * 1024

/**
 * What a file's name says it is, or `null` for a type neither carried nor read.
 *
 * `kind_of` in `dropped.rs`, rule for rule. The name is split on its last dot by hand, so a name
 * that is all extension, `.gitignore`, is read as a name rather than as an extension of nothing.
 */
export function kindOf(path: string): DropKind | null {
  const name = (path.split(/[\\/]/).at(-1) ?? '').toLowerCase()
  if (!name) return null
  const dot = name.lastIndexOf('.')
  const extension = dot > 0 ? name.slice(dot + 1) : null
  if (extension !== null) {
    const media = ATTACHABLE.find(([named]) => named === extension)?.[1]
    if (media) return media === 'application/pdf' ? 'pdf' : 'image'
    if (TEXTUAL.includes(extension)) return 'text'
  }
  const bare = name.startsWith('.') ? name.slice(1) : name
  if (TEXTUAL_NAMES.includes(bare) || TEXTUAL_PREFIXES.some((prefix) => bare.startsWith(prefix))) return 'text'
  return null
}

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
  /** A small picture of a dropped image, as a `data:` URL the preload drew. */
  thumbnail?: string
}

/**
 * What became of one path in a drop, in the order the files were dropped.
 *
 * `named` is a type nothing here takes, whose path is written into the line as text and attaches
 * nothing (DROP-4). `skipped` is a folder (DROP-5), a file too large to carry, or one that could not
 * be read, which the window reports and leaves out.
 */
export type DropOutcome =
  | { kind: 'staged'; file: DroppedFile }
  | { kind: 'named'; text: string }
  | { kind: 'skipped'; name: string; why: 'folder' | 'too-large' | 'unreadable' }

/** What one drop onto a session's conversation came to, as the preload tells the page. */
export interface Drop {
  session: string
  outcomes: DropOutcome[]
}

/**
 * A conversation on its way out of the app, and the one thing that decides whether a
 * message from the window is one.
 *
 * The shape crosses the same three boundaries the layout does — the renderer builds it, the
 * preload types it, the main process writes it down — so the guard lives here with the
 * declaration, for the reason `layout.ts` next door gives: three checks that must agree is
 * three chances to disagree.
 *
 * ## Why the turns cross and not a finished document
 *
 * The renderer could serialize the file itself and hand over a string. It deliberately does
 * not. A string is something the main process can only check the *type* of, and the comment
 * on `bravebot:layout:write` refuses exactly that arrangement: what lands on disk should be
 * something this side composed from a value it understood, never bytes the renderer handed
 * over. So a structured document crosses, [`parseExportRequest`] rebuilds it field by field,
 * and the renderers below run in the main process on the result.
 *
 * ## What an export contains, and what it says about that
 *
 * What the person asked and what the planner replied, and — when the person asked for them —
 * the tool lines between. Never the diffs, the five decision cards or the confined blobs.
 * `plainText` next door makes the argument for the clipboard about those: they are evidence,
 * laid out to be read in place, and a paragraph made out of one reads like a record of the
 * exchange without being one. A whole *file* that did it would make the same false claim at
 * document scale.
 *
 * The tool lines are on the other side of that line, which is why they are offered at all.
 * A tool row is already a sentence — a verb, what it was pointed at, and what came of it —
 * written by the app rather than flattened out of evidence by an export. Reading one on
 * paper tells you the same thing reading it on screen did. It is left out by default anyway,
 * because the common reason to export a session is to show somebody the exchange; whoever
 * wants the work in the file says so in the Export menu, and it goes in.
 *
 * Either way the file ends with a line saying what it left out, rather than trusting the
 * reader to infer it — and that line differs by what was actually carried, so a document
 * cannot claim to have dropped something it contains.
 */

export type ExportFormat = 'txt' | 'md' | 'pdf'

/** One thing somebody said. */
export interface ExportSaid {
  role: 'user' | 'assistant'
  text: string
}

/**
 * One thing the agent did, when the export was asked to carry the work as well as the words.
 *
 * The fields are the tool row's own — see the `tool` case in `Transcript.tsx` — and not a
 * sentence composed in the renderer, for the reason the top of this file gives about the
 * conversation: what lands on disk is composed here, from values this side understood.
 *
 * `note` is `null` where there is no outcome to give: a call still running when the export
 * was taken, and a call replayed out of a stored session, which does not keep what came of
 * one. Both are written as the call alone — see [`toolLine`].
 */
export interface ExportTool {
  role: 'tool'
  verb: string
  target: string
  note: string | null
  failed: boolean
}

/** One row of an export. What was said, and — optionally — what was done between. */
export type ExportTurn = ExportSaid | ExportTool

export interface ExportDocument {
  title: string
  directory: string
  branch: string | null
  turns: ExportTurn[]
}

export interface ExportRequest {
  format: ExportFormat
  document: ExportDocument
}

/**
 * How an export ended.
 *
 * Cancelled is not a failure and has no message: somebody changed their mind, which is not
 * news and must not put anything on screen. `where` comes back on success because it is a
 * path the user has just typed into a native sheet — telling the window about it grants
 * nothing it did not watch happen.
 */
export type ExportOutcome =
  | { status: 'saved'; where: string }
  | { status: 'cancelled' }
  | { status: 'failed'; message: string }

export const EXTENSION: Record<ExportFormat, string> = {
  txt: 'txt',
  md: 'md',
  pdf: 'pdf',
}

/**
 * How much of a session an export will carry.
 *
 * A guard on the offscreen window's memory rather than a policy about what a session may
 * contain: the PDF path lays the whole document out and paginates it in one pass, and there
 * is no partial answer to give if that runs out of room. The text formats are held to the
 * same numbers only because the cap lives in the parse they share, which is a simplification
 * worth naming rather than an opinion about how long a text file may be.
 */
export const MAX_TURNS = 2000
export const MAX_CHARS = 4_000_000

export function isExportFormat(value: unknown): value is ExportFormat {
  return value === 'txt' || value === 'md' || value === 'pdf'
}

/**
 * An export request, or nothing.
 *
 * Nothing is coerced, for the reason `parseLayout` gives: this decides what gets written to
 * a file somebody will keep, and a half-understood message is not a conversation. The
 * returned object is built fresh from the fields that were understood, so a renderer bug
 * cannot smuggle a fourth key through to the window that renders the PDF.
 *
 * An empty conversation comes back `null` rather than as a document with no turns. The
 * button is greyed and the menu items disabled for the same case, and this is the copy of
 * that judgement that cannot be got around.
 */
export function parseExportRequest(value: unknown): ExportRequest | null {
  if (typeof value !== 'object' || value === null) return null
  const { format, document } = value as { format?: unknown; document?: unknown }
  if (!isExportFormat(format)) return null
  if (typeof document !== 'object' || document === null) return null

  const { title, directory, branch, turns } = document as {
    title?: unknown
    directory?: unknown
    branch?: unknown
    turns?: unknown
  }
  if (typeof title !== 'string') return null
  if (typeof directory !== 'string') return null
  if (branch !== null && typeof branch !== 'string') return null
  if (!Array.isArray(turns) || turns.length === 0 || turns.length > MAX_TURNS) return null

  const kept: ExportTurn[] = []
  let chars = 0
  for (const turn of turns) {
    if (typeof turn !== 'object' || turn === null) return null
    const { role } = turn as { role?: unknown }
    if (role === 'tool') {
      const parsed = parseTool(turn)
      if (!parsed) return null
      // A call with no verb is dropped rather than refused, on the same argument as an empty
      // prompt below: there is nothing to write on the line, and the rest of the session is
      // still worth writing down.
      if (parsed.verb.length === 0) continue
      chars += parsed.verb.length + parsed.target.length + (parsed.note?.length ?? 0)
      if (chars > MAX_CHARS) return null
      kept.push(parsed)
      continue
    }
    const { text } = turn as { text?: unknown }
    if (role !== 'user' && role !== 'assistant') return null
    if (typeof text !== 'string') return null
    const trimmed = text.trim()
    // A turn with nothing in it is dropped rather than refused: it is not a malformed
    // message, it is a prompt that was all whitespace, and the rest of the conversation is
    // still worth writing down.
    if (trimmed.length === 0) continue
    chars += trimmed.length
    if (chars > MAX_CHARS) return null
    kept.push({ role, text: trimmed })
  }
  if (kept.length === 0) return null
  // A file of nothing but tool lines is not a conversation, and the window never offers one:
  // `conversation` only adds calls around turns it is already carrying. Refused here for the
  // reason the empty document is — this is the copy of that judgement nothing can get around.
  if (kept.every(isTool)) return null

  return { format, document: { title, directory, branch, turns: kept } }
}

/** Whether a row is a call rather than something somebody said. */
export function isTool(turn: ExportTurn): turn is ExportTool {
  return turn.role === 'tool'
}

/**
 * One field of a tool row, as a single line.
 *
 * A verb, a target and a note are labels the interface draws on one line, never prose, so
 * flattening the whitespace loses nothing. What it gains is that none of the three can carry
 * a line break into a plain-text file or a backtick into a Markdown one — the `.md` file is a
 * document rather than a trust surface, as `toMarkdown` says below, and that holds only while
 * the app's own structural marks are the only ones in it.
 */
function label(value: string): string {
  return value.replace(/\s+/g, ' ').replace(/`/g, "'").trim()
}

/** A tool row, or nothing. */
function parseTool(value: object): ExportTool | null {
  const { verb, target, note, failed } = value as {
    verb?: unknown
    target?: unknown
    note?: unknown
    failed?: unknown
  }
  if (typeof verb !== 'string') return null
  if (typeof target !== 'string') return null
  if (note !== null && typeof note !== 'string') return null
  if (typeof failed !== 'boolean') return null
  return {
    role: 'tool',
    verb: label(verb),
    target: label(target),
    note: note === null ? null : label(note),
    failed,
  }
}

/** What each side is called in an exported file. */
const SPEAKER: Record<ExportSaid['role'], string> = {
  user: 'You',
  assistant: 'Brave Bot',
}

/**
 * The line an export ends with.
 *
 * See the note at the top of this file: an export carries the conversation, optionally the
 * calls, and never the evidence — and saying so is cheaper than a reader discovering it. Two
 * sentences rather than one, because a document that carried the tool lines and then claimed
 * to have dropped them would be the footer lying about the pages above it.
 */
export const OMITTED = 'Tool calls, diffs and approvals are not part of this export.'
export const OMITTED_WITH_TOOLS = 'Diffs, approvals and confined output are not part of this export.'

/** Which of the two the footer is, for a document that has been parsed. */
export function omitted(document: ExportDocument): string {
  return document.turns.some(isTool) ? OMITTED_WITH_TOOLS : OMITTED
}

/**
 * A call, as one line: what it did, to what, and what came of it.
 *
 * The bracket around the target is the transcript's own, and the trailing em dash is where
 * the note sits on screen. A call with no note is written as the call alone: most of them
 * come from a stored session, whose record keeps what a turn did and not what came of it,
 * and the rest were still running when the export was taken. Nothing is added in place of
 * the outcome — an ellipsis would claim the call was in flight, which is wrong for the
 * common case, and any word at all would be an outcome nobody reported.
 */
export function toolLine(turn: ExportTool): string {
  const head = turn.target ? `${turn.verb} (${turn.target})` : turn.verb
  if (turn.note === null) return head
  return turn.failed ? `${head} — failed: ${turn.note}` : `${head} — ${turn.note}`
}

/**
 * When it was written, in a fixed locale.
 *
 * Not the machine's, so one session exported twice on two computers produces the same bytes.
 */
function when(at: number): string {
  return new Date(at).toLocaleString('en-GB', { dateStyle: 'long', timeStyle: 'short' })
}

/** `directory · branch`, or just the directory where there is no branch. */
export function where(document: ExportDocument): string {
  return document.branch ? `${document.directory} · ${document.branch}` : document.directory
}

/**
 * The session's own description, as the window's header already gives it: the same three
 * facts in the same order, because the top of the file should be the top of the window.
 */
export function toPlainText(document: ExportDocument, at: number): string {
  const rule = '─'.repeat(60)
  const lines = [document.title, where(document), `Exported ${when(at)}`, '', rule, '']
  for (const turn of document.turns) {
    // A call is indented under the exchange rather than given a speaker line. It is not a
    // third party to the conversation, it is what happened between two things that were
    // said, and the margin is what says so on a page with no colour in it.
    if (isTool(turn)) lines.push(`    · ${toolLine(turn)}`, '')
    else lines.push(SPEAKER[turn.role], turn.text, '')
  }
  lines.push(rule, omitted(document))
  return `${lines.join('\n')}\n`
}

/**
 * The same conversation as Markdown.
 *
 * The assistant's words go in untouched, because they already are Markdown — that is what
 * the bubble on screen is rendering. A prompt is not, so it is quoted: a person's own text
 * is not a document, and a prompt that happened to begin with `#` would otherwise become
 * somebody's heading. That is presentation and not sanitisation, and the difference matters
 * enough to say out loud — a `.md` file is a document, not a trust surface, and none of the
 * app's structural markings survive being written to one.
 */
export function toMarkdown(document: ExportDocument, at: number): string {
  const quoted = (text: string): string =>
    text
      .split('\n')
      .map((line) => (line.length > 0 ? `> ${line}` : '>'))
      .join('\n')

  const out = [
    `# ${document.title}`,
    '',
    `\`${where(document)}\` · exported ${when(at)}`,
    '',
    '---',
    '',
  ]
  for (const turn of document.turns) {
    // A list item, because a run of calls is a list of what was done and a reader scanning
    // for the reply should be able to skip it in one movement. Inside a code span for the
    // same reason the target is bracketed on screen: it is a path and an outcome, not prose.
    if (isTool(turn)) {
      out.push(`- \`${toolLine(turn)}\``, '')
      continue
    }
    out.push(`**${SPEAKER[turn.role]}**`, '')
    out.push(turn.role === 'user' ? quoted(turn.text) : turn.text, '')
  }
  out.push('---', '', `*${omitted(document)}*`)
  return `${out.join('\n')}\n`
}

/**
 * What to call the file, before the user gets a say.
 *
 * A session's title is written by the agent from the first prompt, which makes this the one
 * piece of model-influenced text that reaches a native control. The rule about renderable
 * content in `commands.ts` is about menu *labels* and does not reach a filename field, but
 * what a title can do to a path does: a separator escapes the directory, a leading dot hides
 * the file, and a bidirectional override makes `notes.txt.app` draw as something else
 * entirely. So this strips rather than escapes, and falls back to a name of our own when
 * nothing survives.
 */
export function suggestedFilename(title: string, format: ExportFormat): string {
  const cleaned = title
    .normalize('NFC')
    // Path separators and the drive marker, then everything unprintable — including the
    // format characters that reorder what a person reads.
    .replace(/[/:\\]/g, ' ')
    .replace(/[\u0000-\u001f\u007f]/g, ' ')
    .replace(/\p{Cf}/gu, '')
    .replace(/\s+/g, ' ')
    .trim()
    .replace(/^\.+/, '')
    .replace(/\.+$/, '')
    .slice(0, 80)
    .trim()
  return `${cleaned || 'Brave Bot session'}.${EXTENSION[format]}`
}

/**
 * The path the user chose, with the extension it needs.
 *
 * Appends rather than replaces. Somebody who typed `notes.v2` into the sheet gets
 * `notes.v2.md`: the OS handed back exactly what a person asked for, and rewriting the part
 * they typed would be the app overruling them about the name of their own file.
 */
export function withExtension(path: string, format: ExportFormat): string {
  const wanted = `.${EXTENSION[format]}`
  return path.toLowerCase().endsWith(wanted) ? path : `${path}${wanted}`
}

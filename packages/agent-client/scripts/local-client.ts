/**
 * A small client that drives one fresh session of a local bravebot-rpc through the typed client.
 *
 *   node dist/scripts/local-client.js --rpc <bravebot-rpc> --directory <dir> --trust yes|no \
 *        [--decide approve|reject|prompt] <prompt>
 *
 * The child inherits this process's environment, so it reaches the model the way `bravebot-rpc`
 * normally does. Startup trust is never assumed: `--trust` is required. Approvals are answered
 * only as `--decide` says; with `prompt` a person is asked on the terminal, and without a
 * terminal the answer is reject.
 *
 * This is a diagnostic dump, not a display surface. Each released payload is printed as one line
 * of JSON with control, zero-width and bidirectional characters escaped; it is not rendered or marked for
 * reading as the application's own text. A user question is declined, not answered.
 */

import { parseArgs } from 'node:util'
import type { AgentSession, ViewState } from '../src/common/index.js'
import { connectStdio } from '../src/node/index.js'
import { yesNoAsker } from './ask.js'

const { values, positionals } = parseArgs({
  allowPositionals: true,
  options: {
    rpc: { type: 'string' },
    directory: { type: 'string' },
    trust: { type: 'string' },
    decide: { type: 'string', default: 'prompt' },
  },
})
const prompt = positionals.join(' ')
if (!values.rpc || !values.directory || !prompt || (values.trust !== 'yes' && values.trust !== 'no')) {
  console.error('usage: local-client --rpc <bravebot-rpc> --directory <dir> --trust yes|no [--decide approve|reject|prompt] <prompt>')
  process.exit(2)
}
if (values.decide !== 'approve' && values.decide !== 'reject' && values.decide !== 'prompt') {
  console.error('--decide must be approve, reject or prompt')
  process.exit(2)
}

/**
 * Text with every character a terminal could act on written as an escape: the control characters,
 * the format characters (zero-width, bidirectional, tag and invisible-operator characters), the line
 * and paragraph separators, and the variation selectors and combining grapheme joiner that hide text.
 * Everything the bridge supplies is printed through this, not only released payloads.
 */
function escape(text: string): string {
  return text.replace(/[\p{Cc}\p{Cf}\p{Zl}\p{Zp}\u034f\ufe00-\ufe0f\u{e0100}-\u{e01ef}]/gu, (character) => {
    // Escapes a JSON reader understands: a character above U+FFFF is written as its surrogate pair.
    return [...character]
      .flatMap((point) => {
        const code = point.codePointAt(0)!
        return code > 0xffff ? [0xd800 + ((code - 0x10000) >> 10), 0xdc00 + ((code - 0x10000) & 0x3ff)] : [code]
      })
      .map((unit) => '\\u' + unit.toString(16).padStart(4, '0'))
      .join('')
  })
}

/** One line of JSON, escaped. */
const plain = (value: unknown): string => escape(JSON.stringify(value) ?? 'null')

const describe = (error: unknown): string => (error instanceof Error ? error.message : String(error))

const mode = values.decide
const connection = connectStdio({
  command: values.rpc,
  env: process.env,
  workspaces: [{ id: 'project', name: 'Project', directory: values.directory }],
})

let asker: ReturnType<typeof yesNoAsker> | undefined

async function decision(question: NonNullable<ViewState['pending']>): Promise<'approve' | 'reject'> {
  if (mode !== 'prompt') return mode
  // No terminal: nobody can approve, so the answer is reject.
  if (!process.stdin.isTTY) return 'reject'
  asker ??= yesNoAsker(process.stdin, process.stderr)
  const yes = await asker.ask(`${escape(question.kind)} question ${plain(question.data)}\napprove? [y/N] `)
  return yes ? 'approve' : 'reject'
}

/** Answer the question on screen as the options say. A failure is reported and the turn carries on. */
async function respond(session: AgentSession, question: NonNullable<ViewState['pending']>): Promise<void> {
  if (!question.supported) {
    console.log(`cancelling: a ${escape(question.kind)} question cannot be answered here`)
    await session.cancel()
  } else if (question.kind === 'ask') {
    console.log('declining the question')
    await session.answer(question.request, [null])
  } else {
    const choice = await decision(question)
    console.log(`${choice} ${escape(question.kind)} request ${question.request}`)
    await session.decide(question.request, choice)
  }
}

const turnOver = (view: ViewState): boolean =>
  view.ended !== null || (view.turn >= 1 && ['completed', 'failed', 'cancelled'].includes(view.status))

async function main(): Promise<number> {
  const info = await connection.client.describe()
  console.log(`runtime ${escape(info.version ?? '?')} build ${escape(info.build ?? '?')} with session view ${info.sessionView.version}`)
  const session = await connection.client.createSession({ workspace: 'project' })
  console.log(`session ${escape(session.id)} status ${session.view.status}`)

  let shown = 0
  let answered = -1
  let end!: (view: ViewState) => void
  const over = new Promise<ViewState>((resolve) => (end = resolve))
  session.subscribe((view) => {
    console.log(`view ${view.sequence} turn ${view.turn} status ${view.status}`)
    for (const row of view.rows.slice(shown)) {
      console.log(`row ${row.id} ${row.kind} ${escape(row.event ?? '-')} payload ${plain(row.data)}`)
    }
    shown = view.rows.length
    if (view.pending && view.pending.request !== answered) {
      answered = view.pending.request
      respond(session, view.pending).catch((error: unknown) => {
        console.error(`could not answer: ${escape(describe(error))}`)
      })
    }
    if (turnOver(view)) end(view)
  })

  await session.answerTrust(values.trust === 'yes')
  await session.send(prompt)
  const finished = await over
  console.log(`turn ended ${finished.ended ? `view ended (${finished.ended.reason})` : finished.status}`)
  const closed = await session.close()
  console.log(`closed: view detached ${closed.viewDetached}, worker ${closed.workerTerminated}, saved ${closed.saved}`)
  return finished.status === 'completed' ? 0 : 1
}

let code = 1
try {
  code = await main()
} catch (error) {
  console.error(escape(describe(error)))
} finally {
  asker?.close()
  await connection.dispose(1000).catch(() => undefined)
}
// Exit once everything written has been taken by the reader; exiting at once drops what a slow
// reader of a pipe has not yet read.
process.stdout.write('', () => process.stderr.write('', () => process.exit(code)))

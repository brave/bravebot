/**
 * A client program for `bravebot-host` that a person or a test types commands into: the host test
 * client of stage 5a. Its standard input and output are the channel the host made for it, so it
 * takes its commands from `--input` (the terminal by default) and writes what it sees to standard
 * error, each line starting with `host-client NAME:`.
 *
 *   bravebot-host --sessions-root <root> --workspace <dir> --client desk --answers <kinds> \
 *     --program <node> --arg dist/scripts/host-client.js --arg --name --arg desk \
 *     --arg --workspace --arg <dir> [--arg --input --arg <file or fifo>]
 *
 * Commands, one per line:
 *
 *   list            the sessions the host holds, and whether this connection is attached to each
 *   new             open a session in the workspace
 *   attach ID       join a session the host holds, whether or not another client is attached
 *   send TEXT       send a message to the session
 *   trust yes|no    answer the startup trust question without waiting to be asked
 *   decide N approve|reject  answer request N without waiting to be shown it
 *   cancel          cancel the turn on screen
 *   close           close the session
 *   quit            end this client
 *
 * The startup trust question is shown as a typed yes or no when the host let this client answer
 * trust, and the next line `yes` or `no` answers it. Where the client may not answer trust the
 * line says to answer it on the computer the host runs on. A question a turn asks is shown the
 * same way and answered with `approve` or `reject`, where the client may answer its kind, and is
 * marked as one to answer on another device where it may not.
 *
 * This is a test and development program. Everything it prints is escaped, and it reads no
 * credential or setting.
 */

import { closeSync, constants, createReadStream, fstatSync, openSync } from 'node:fs'
import { Socket } from 'node:net'
import { basename, isAbsolute } from 'node:path'
import { createInterface } from 'node:readline'
import { isatty, ReadStream as TtyReadStream } from 'node:tty'
import { parseArgs } from 'node:util'
import { RpcAgentClient } from '../src/common/client.js'
import type { AgentSession, AnswerKind, ViewState } from '../src/common/index.js'

const { values } = parseArgs({
  options: {
    name: { type: 'string' },
    workspace: { type: 'string' },
    input: { type: 'string', default: '/dev/tty' },
  },
})
if (values.name === undefined || values.workspace === undefined || !isAbsolute(values.workspace)) {
  process.stderr.write('usage: host-client --name NAME --workspace /absolute/dir [--input FILE]\n')
  process.exit(2)
}
const { name, workspace } = values
const input = values.input ?? '/dev/tty'

/** Text with every character a terminal could act on written as an escape. */
function escape(text: string): string {
  return text.replace(/[\p{Cc}\p{Cf}\p{Zl}\p{Zp}\u034f\ufe00-\ufe0f\u{e0100}-\u{e01ef}]/gu, (character) =>
    [...character]
      .flatMap((point) => {
        const code = point.codePointAt(0)!
        return code > 0xffff ? [0xd800 + ((code - 0x10000) >> 10), 0xdc00 + ((code - 0x10000) & 0x3ff)] : [code]
      })
      .map((unit) => '\\u' + unit.toString(16).padStart(4, '0'))
      .join(''),
  )
}

const say = (line: string): void => void process.stderr.write(`host-client ${name}: ${escape(line)}\n`)
const plain = (value: unknown): string => JSON.stringify(value) ?? 'null'
const describe = (error: unknown): string => {
  const code = error instanceof Error ? (error as { code?: unknown }).code : undefined
  return error instanceof Error ? `${typeof code === 'string' ? `${code}: ` : ''}${error.message}` : String(error)
}

const client = new RpcAgentClient(
  {
    write(line) {
      process.stdout.write(line)
    },
  },
  {
    workspaces: [{ id: workspace, name: basename(workspace) || workspace, directory: workspace }],
    onDiagnostic: (message) => say(`diagnostic: ${message}`),
    onClosed: (detail) => {
      say(`the host ended the connection: ${detail}`)
      process.exit(1)
    },
    deadlines: {
      ms: 30_000,
      schedule(ms, fire) {
        const timer = setTimeout(fire, ms)
        return () => clearTimeout(timer)
      },
    },
  },
)
process.stdin.setEncoding('utf8')
process.stdin.on('data', (chunk: string) => client.receive(chunk))
process.stdin.on('end', () => client.transportClosed('the host closed the channel'))

let answers: readonly AnswerKind[] = []
let session: AgentSession | null = null
/** What the next line answers instead of being a command. */
let asking: { kind: 'trust' } | { kind: 'question'; request: number; ask: boolean } | null = null
let announcedTrust = false
let announcedQuestion = -1

/** Whether this client may answer a question of the kind the view calls `kind`. */
const mayAnswer = (kind: string): boolean => (answers as readonly string[]).includes(kind)

function show(view: ViewState): void {
  say(`view ${view.sequence} turn ${view.turn} status ${view.status} actor ${view.actor ?? '-'}`)
  const pending = view.pending
  // A question another client answered, or a turn ended, is no longer waiting for a line.
  if (pending === null && asking?.kind === 'question') asking = null
  if (pending === null) return
  if (pending.request === announcedQuestion) return
  announcedQuestion = pending.request
  if (!pending.supported || !mayAnswer(pending.kind)) {
    say(`answer on another device: a ${pending.kind} question, request ${pending.request}`)
    return
  }
  asking = { kind: 'question', request: pending.request, ask: pending.kind === 'ask' }
  say(`question ${pending.kind} ${pending.request}: ${plain(pending.data)}; type ${pending.kind === 'ask' ? 'reject to decline' : 'approve or reject'}`)
}

function begin(held: AgentSession): void {
  session = held
  say(`session ${held.id} status ${held.view.status}`)
  held.subscribe(show)
  show(held.view)
  if (held.startupTrust !== null && !announcedTrust) {
    announcedTrust = true
    if (mayAnswer('trust')) {
      asking = { kind: 'trust' }
      say(`trust question: ${plain(held.startupTrust)}; type yes or no`)
    } else {
      say('this folder is not trusted yet; answer the question on the computer the host runs on')
    }
  }
}

async function command(line: string): Promise<boolean> {
  const [verb = '', ...rest] = line.trim().split(/\s+/)
  const text = line.trim().slice(verb.length).trim()
  if (verb === 'quit') return false
  if (verb === 'list') {
    const held = await client.instance()
    answers = held.answers
    say(`answers ${held.answers.join(',') || 'none'}`)
    for (const one of held.sessions) say(`held ${one.session} ${one.state} attached ${one.attached} working ${one.working}`)
    return true
  }
  if (verb === 'new') {
    await client.describe()
    answers = (await client.instance()).answers
    announcedTrust = false
    begin(await client.createSession({ workspace }))
    return true
  }
  if (verb === 'attach') {
    const id = rest[0]
    if (id === undefined) return say('attach needs a session'), true
    answers = (await client.instance()).answers
    announcedTrust = false
    begin(await client.attach(id))
    return true
  }
  const held = session
  if (held === null) return say('no session; use new or attach'), true
  if (verb === 'trust') {
    const word = rest[0]
    if (word !== 'yes' && word !== 'no') return say('trust needs yes or no'), true
    await held.answerTrust(word === 'yes')
    say(`trust answered ${word}`)
  } else if (verb === 'decide') {
    // An answer given without waiting to be shown the question, for a question this client was not
    // shown or one that may already be answered.
    const request = Number(rest[0])
    const word = rest[1]
    if (!Number.isInteger(request) || (word !== 'approve' && word !== 'reject')) return say('decide needs a request and approve or reject'), true
    await held.decide(request, word)
    say(`${word} request ${request}`)
  } else if (verb === 'send') {
    const sent = await held.send(text)
    say(`sent turn ${sent.turn} target ${sent.target}`)
  } else if (verb === 'cancel') {
    say(`cancel ${plain(await held.cancel())}`)
  } else if (verb === 'close') {
    say(`closed ${plain(await held.close())}`)
    session = null
  } else {
    say(`unknown command ${verb}`)
  }
  return true
}

/** Whether `line` is the answer to `question`, and not a command. */
function isAnswer(line: string, question: NonNullable<typeof asking>): boolean {
  const word = line.trim().toLowerCase()
  return question.kind === 'trust' ? word === 'yes' || word === 'no' : word === 'approve' || word === 'reject'
}

/** Answer the question on screen with `line`, which is an answer to it. */
async function answer(line: string, question: NonNullable<typeof asking>): Promise<void> {
  const word = line.trim().toLowerCase()
  const held = session
  if (held === null) return
  if (question.kind === 'trust') {
    await held.answerTrust(word === 'yes')
    say(`trust answered ${word}`)
    return
  }
  if (question.ask) {
    // This client has no way to type an answer to the planner's questions, so it declines.
    await held.answer(question.request, [null])
    say(`declined request ${question.request}`)
    return
  }
  await held.decide(question.request, word === 'approve' ? 'approve' : 'reject')
  say(`${word} request ${question.request}`)
}

/**
 * The commands' source. A named pipe or a terminal is read through the event loop, not a thread
 * that waits in a read, so ending the client does not wait for a person to type or close it.
 */
function openInput(path: string): NodeJS.ReadableStream {
  const opened = openSync(path, constants.O_RDONLY | constants.O_NONBLOCK)
  if (fstatSync(opened).isFIFO()) {
    // Reopened for reading and writing so that a named pipe nobody has written to yet is not at its
    // end. The client then ends on `quit` or when the host ends its channel.
    try {
      const both = openSync(path, constants.O_RDWR | constants.O_NONBLOCK)
      closeSync(opened)
      return new Socket({ fd: both, readable: true, writable: false })
    } catch {
      // A pipe that cannot be reopened, such as one a shell made for `<(...)`, ends when its writer does.
      return new Socket({ fd: opened, readable: true, writable: false })
    }
  }
  if (isatty(opened)) {
    closeSync(opened)
    return new TtyReadStream(openSync(path, constants.O_RDONLY))
  }
  closeSync(opened)
  return createReadStream(path, { encoding: 'utf8' })
}

const lines = createInterface({ input: openInput(input) })
// One line at a time: a command that waits on the host is finished before the next is read.
let busy: Promise<void> = Promise.resolve()
lines.on('line', (line) => {
  busy = busy.then(async () => {
    if (ending) return
    try {
      const question = asking
      if (question !== null && isAnswer(line, question)) {
        asking = null
        await answer(line, question)
      } else if (line.trim() !== '' && !(await command(line))) endClient()
    } catch (error) {
      say(`failed: ${describe(error)}`)
    }
  })
})
lines.on('close', () => void busy.then(endClient))

let ending = false

function endClient(): void {
  if (ending) return
  // Nothing typed after `quit` is run.
  ending = true
  lines.close()
  // Exits once the line is written, or after a second if the reader of standard error is not reading.
  setTimeout(() => process.exit(0), 1000).unref()
  process.stderr.write(`host-client ${name}: ending\n`, () => process.exit(0))
}

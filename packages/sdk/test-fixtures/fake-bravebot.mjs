#!/usr/bin/env node
// A stand-in for the `bravebot` command line that writes what a scenario says, so the library's
// handling of the stream can be tested without a model. FAKE_SCENARIO picks the scenario.
// FAKE_ARGV_FILE receives the command line it was started with, and FAKE_STDIN_FILE each line it read.
import { appendFileSync, writeFileSync } from 'node:fs'

const scenario = process.env.FAKE_SCENARIO ?? 'echo'
if (process.env.FAKE_ARGV_FILE) writeFileSync(process.env.FAKE_ARGV_FILE, JSON.stringify(process.argv.slice(2)))

const result = (reply, extra = {}) => ({
  schema: 1,
  ok: true,
  status: 0,
  reason: 'done',
  identifier: null,
  message: null,
  reply,
  model: 'fake',
  agent: null,
  session: null,
  steps: 1,
  tokens: { total: 1, output: 1, context: 1, cache_read: 0, cache_written: 0 },
  calls: [],
  refusals: [],
  notices: [],
  structured: null,
  future_field: 'kept',
  ...extra,
})
const write = (object) => process.stdout.write(`${JSON.stringify(object)}\n`)
const hostile = '"},"ok":false,"status":9,"x":"\n{"event":"call"}\u2028 \\ \u001b[2K ✓ 🙂'

if (scenario === 'echo') {
  const prompt = process.argv[process.argv.indexOf('-p') + 1]
  write({ schema: 1, event: 'call', tool: 'list_files', target: '.', refused: false })
  write({ schema: 1, event: 'future_kind', anything: [1, 2] })
  write({ schema: 1, event: 'usage', tokens: { total: 2, output: 1, context: 1, cache_read: 0, cache_written: 0 } })
  write(result(`echo: ${prompt}`))
} else if (scenario === 'hostile') {
  write({ schema: 1, event: 'call', tool: 'read_file', target: hostile, refused: false })
  write(result(hostile, { message: hostile }))
} else if (scenario === 'refused') {
  write({ schema: 1, event: 'refusal', gate: 'action', principle: 'integrity-gate', reason: 'injection blocked' })
  write(result('', {
    ok: false,
    status: 4,
    reason: 'refusal',
    identifier: 'BB1004',
    refusals: [{ gate: 'action', principle: 'integrity-gate', reason: 'injection blocked' }],
  }))
  process.exitCode = 4
} else if (scenario === 'no-result') {
  write({ schema: 1, event: 'usage', tokens: { total: 2, output: 1, context: 1, cache_read: 0, cache_written: 0 } })
} else if (scenario === 'half-line') {
  process.stdout.write('{"schema":1,"event":"usa')
} else if (scenario === 'garbage') {
  process.stdout.write('this is not json SECRET-WORDS\n')
  setInterval(() => undefined, 1000)
} else if (scenario === 'not-an-object') {
  process.stdout.write('[1,2]\n')
  setInterval(() => undefined, 1000)
} else if (scenario === 'hang') {
  write({ schema: 1, event: 'call', tool: 'list_files', target: '.', refused: false })
  setInterval(() => undefined, 1000)
} else if (scenario === 'stderr') {
  process.stderr.write('progress \u2713 line\n')
  write(result('quiet'))
} else if (scenario === 'chunks') {
  const text = `${JSON.stringify(result('héllo ✓ 🙂'))}\n`
  const bytes = Buffer.from(text, 'utf8')
  let at = 0
  const timer = setInterval(() => {
    process.stdout.write(bytes.subarray(at, at + 3))
    at += 3
    if (at >= bytes.length) clearInterval(timer)
  }, 2)
} else if (scenario === 'lines') {
  // Reads the lines of CLI-36: a prompt gets a result, a prompt naming ASK asks a question first.
  let turn = 0
  let questionId = 0
  const waiting = []
  const pending = []
  // Lines end at \n only, as in CLI-36: readline would also split at U+2028.
  let buffered = ''
  process.stdin.setEncoding('utf8')
  process.stdin.on('data', (chunk) => {
    buffered += chunk
    const parts = buffered.split('\n')
    buffered = parts.pop() ?? ''
    for (const part of parts) onLine(part)
  })
  const onLine = (line) => {
    if (process.env.FAKE_STDIN_FILE) appendFileSync(process.env.FAKE_STDIN_FILE, `${line}\n`)
    let object
    try {
      object = JSON.parse(line)
    } catch {
      write({ schema: 1, event: 'input_refused', line: 0, reason: 'not_json' })
      return
    }
    if (object.type === 'prompt') {
      pending.push(object.text)
      if (pending.length === 1) next()
    } else if (object.type === 'answer') {
      const at = waiting.findIndex((one) => one === object.id)
      if (at < 0) {
        write({ schema: 1, event: 'input_refused', line: 0, reason: 'no_such_question' })
        return
      }
      waiting.splice(at, 1)
      turn += 1
      write(result(`answered ${JSON.stringify(object.choices)}`))
      pending.shift()
      next()
    } else if (object.type === 'stop') {
      write(result('', { ok: false, status: 1, reason: 'stopped' }))
    } else {
      write({ schema: 1, event: 'input_refused', line: 0, reason: 'unknown_type' })
    }
  }
  const next = () => {
    const text = pending[0]
    if (text === undefined) return
    if (text.includes('ASK')) {
      questionId += 1
      waiting.push(questionId)
      write({
        schema: 1,
        event: 'question',
        id: questionId,
        questions: [
          { header: 'Cache', question: 'Which layer?', choices: [{ label: 'HTTP', detail: null }, { label: 'Query', detail: 'db' }], multiple: false },
          { header: 'Scope', question: 'Which?', choices: [], multiple: true },
        ],
      })
      return
    }
    turn += 1
    write(result(`Reply ${turn} to: ${text}`))
    pending.shift()
    next()
  }
} else {
  process.stderr.write(`unknown scenario ${scenario}\n`)
  process.exitCode = 70
}

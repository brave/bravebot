// `/schedule` typed into the composer (CMD-1, SCHEDULE-8), as the desktop window handles it.
//
// The bridge cuts the line and carries it out with the code `bravebot schedule` runs
// (crates/ui-bridge/src/schedule_command.rs). What the window owns is which lines go there at all,
// and that the answer is a row to read and never part of what the model is sent. The faults these
// reject are a longer word taken for the command, a line from the queue of another kind or from
// model output being asked as the command, a result drawn as markup, and an answer that lands in
// the conversation the model keeps.
import test from 'node:test'
import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import { buildSync } from 'esbuild'
import { createRequire } from 'node:module'

const require = createRequire(import.meta.url)
const load = (contents) => {
  const source = buildSync({ stdin: { contents, resolveDir: process.cwd(), loader: 'ts' }, bundle: true, write: false, platform: 'node', format: 'cjs', external: ['electron'] }).outputFiles[0].text
  const module = { exports: {} }
  new Function('require', 'module', 'exports', source)(require, module, module.exports)
  return module.exports
}

const shared = load("export * from './src/shared/schedule'")
const events = load("export { apply } from './src/renderer/apply'")
const transcript = load("export * from './src/renderer/transcript'")

const live = () => ({
  entries: [{ kind: 'user', id: 'u0', text: 'opening prompt' }],
  turns: {}, queued: [], interjected: [], running: false, queuePaused: false,
  summary: { id: null, title: 't', project: '/p', branch: null, directory: '/p' },
  archived: 0, awaitingOrdinal: null, staging: { made: 0, staged: [] }, agents: [],
})
const heard = (before, message) => {
  let after = before
  events.apply(message, (action) => { after = typeof action === 'function' ? action(after) : action }, () => {}, () => {})
  return after
}

test('only the command word, alone or with an argument, is the command', () => {
  assert.equal(shared.scheduleArgument('/schedule'), '')
  assert.equal(shared.scheduleArgument('  /schedule list  '), 'list')
  assert.equal(shared.scheduleArgument('/schedule add --cron "0 9 * * *" --prompt "x"'), 'add --cron "0 9 * * *" --prompt "x"')
})

test('a longer word, or a sentence that mentions the command, is a prompt', () => {
  for (const prompt of ['/scheduled', '/schedulelist', 'please /schedule list', 'run /schedule', '', 'schedule list']) {
    assert.equal(shared.scheduleArgument(prompt), null, prompt)
  }
})

test('a result that arrives later is drawn as a row of the transcript, with its lines kept', () => {
  const after = heard(live(), { event: 'schedule.ran', session: 's', data: { lines: ['schedule 3 ran, status 0', 'session s'], refused: false, running: false } })
  const row = after.entries.at(-1)
  assert.equal(row.kind, 'narration')
  assert.equal(row.text, 'schedule 3 ran, status 0\nsession s')
  assert.equal(row.refused, undefined)
  assert.equal(after.running, false, 'a result starts no turn')
})

test('a refused answer is marked, and its sentence is the row', () => {
  const row = transcript.scheduleAnswered({ lines: ['not allowed'], refused: true, running: false })
  assert.equal(row.kind, 'narration')
  assert.equal(row.text, 'not allowed')
  assert.equal(row.refused, true)
})

test('the row is plain text that keeps its line breaks and is not an entry the model is sent', () => {
  const css = readFileSync('src/renderer/styles/activity.css', 'utf8')
  assert.match(css, /\.narration \{[^}]*white-space: pre-wrap/)
  // Markup and markdown in a line: the row keeps both as the characters they are.
  const markup = '<img src=x onerror=alert(1)>'
  const row = transcript.scheduleAnswered({ lines: [markup, '**b**'], refused: false, running: false })
  assert.equal(row.text, `${markup}\n**b**`)
  // `narration` is the kind the transcript draws as text; no markdown or HTML path is taken for it.
  const drawn = readFileSync('src/renderer/components/Transcript.tsx', 'utf8')
  assert.match(drawn, /case 'narration':\s*return <div className=\{entry\.refused \? 'narration refused' : 'narration'\}><Icon name="file-text" \/><span><FailureSentence/)
})

test('the composer asks for the command only from the text a person typed, through one function', () => {
  const turn = readFileSync('src/renderer/use-turn.ts', 'utf8')
  // The recogniser is applied to the draft in the three places the box sends a line from.
  assert.equal([...turn.matchAll(/scheduleArgument\(/g)].length, 3, 'submit, the interject key and the queue key')
  // A line leaving the queue is asked again only where the person's line was held for the turn.
  assert.match(turn, /message\.schedule !== undefined\s*\?\s*schedule\(handle, message\.schedule, message\.prompt, true\)\s*:\s*send\(/)
  // The window holds a writing form for the turn on the bridge's refusal, not on a reading of the line.
  assert.match(turn, /error\.code === 'turn_in_flight'/)
  assert.doesNotMatch(turn.slice(turn.indexOf('const schedule = ')), /reads_only|readsOnly|startsWith\('(add|remove)/)
  // Nothing outside the box's handlers makes a queued schedule line.
  const queuing = [...turn.matchAll(/schedule: argument/g)]
  assert.equal(queuing.length, 1)
})

test('the bridge method is one the window may send, and is a method the main process does not gate by directory', () => {
  const index = readFileSync('src/main/index.ts', 'utf8')
  assert.match(index, /'changelog\.read',\s*'schedule\.command',/)
})

// `/loop` typed into the composer (CMD-1, LOOP-3), as the desktop window handles it.
//
// The bridge reads the argument with the terminal's own parser and holds the loops
// (crates/ui-bridge/src/loop_command.rs). What the window owns is which lines go there at all, that
// a start refused while a turn runs waits in the queue (LOOP-6, CMD-8), that a `/loop stop` typed
// behind a waiting `/loop` waits too (LOOP-11), and the rows that say a loop started, ticked and
// ended (LOOP-13). The faults these reject are a longer word taken for the command, a line sent as a
// prompt, a stop that jumps a waiting start, a tick or ending drawn without its number, and an
// answer drawn under the first tick's reply.
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

const shared = load("export * from './src/shared/loop'")
const staging = load("export { exitOf } from './src/renderer/staging'")
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
const started = (number, lines = [`Loop ${number} started: every 10s.`], adjusted = null) =>
  ({ lines, refused: false, number, pace: 'every 10s', adjusted })

test('only the command word, alone or with an argument, is the command', () => {
  assert.equal(shared.loopArgument('/loop'), '')
  assert.equal(shared.loopArgument('  /loop 10s say hi  '), '10s say hi')
  assert.equal(shared.loopArgument('/loop stop 2'), 'stop 2')
  for (const prompt of ['/looping', '/loops', 'please /loop 5m x', 'loop 5m x', '']) {
    assert.equal(shared.loopArgument(prompt), null, prompt)
  }
})

test('the composer routes /loop lines to the loop exit and leaves a longer word as a prompt', () => {
  assert.deepEqual(staging.exitOf(undefined, '/loop 10s say hi'), { to: 'loop', argument: '10s say hi', line: '/loop 10s say hi', staged: [] })
  assert.deepEqual(staging.exitOf(undefined, '/loop'), { to: 'loop', argument: '', line: '/loop', staged: [] })
  assert.equal(staging.exitOf(undefined, '/looping').to, 'prompt')
  // A `/schedule` line is still the schedule exit.
  assert.equal(staging.exitOf(undefined, '/schedule list').to, 'schedule')
})

test('a tick row carries its loop number, its own, and the quiet ticks only where there are any (LOOP-13)', () => {
  const first = heard(live(), { event: 'loop.tick', session: 's', data: { number: 2, tick: 1, quiet: 0 } })
  const row = first.entries.at(-1)
  assert.equal(row.kind, 'loop-tick')
  assert.equal(row.number, 2)
  assert.equal(row.text, 'Loop 2, tick 1')
  assert.equal(first.running, false, 'the turn is started by the bridge, and says so with turn.started')
  assert.equal(transcript.loopTicked({ number: 2, tick: 5, quiet: 3 }).text, 'Loop 2, tick 5 · 3 quiet')
  // The tick's turn is drawn below it, as a watch's is.
  const turned = heard(first, { event: 'turn.started', session: 's', data: { turn: 4 } })
  assert.deepEqual(turned.entries.slice(-2).map((entry) => entry.kind), ['loop-tick', 'turn-start'])
})

test('an ending says which loop ended, or how many a stop ended (LOOP-11)', () => {
  const one = heard(live(), { event: 'loop.ended', session: 's', data: { number: 3, reason: 'finished' } })
  assert.equal(one.entries.at(-1).kind, 'narration')
  assert.match(one.entries.at(-1).text, /^Loop 3 ended: /)
  assert.match(transcript.loopEnded({ reason: 'stopped', count: 2 }).text, /^2 loops ended: stopped/)
  assert.match(transcript.loopEnded({ reason: 'stopped', count: 1 }).text, /^The loop ended: stopped/)
  assert.match(transcript.loopEnded({ number: 1, reason: 'aged' }).text, /^Loop 1 ended: /)
  assert.match(transcript.loopEnded({ number: 1, reason: 'failed', message: 'No model.' }).text, /No model\.$/)
  const armed = heard(live(), { event: 'loop.armed', session: 's', data: { number: 1, after: '20m' } })
  assert.match(armed.entries.at(-1).text, /^Loop 1 started: .*20m/)
})

test('the answer to a start is drawn before the first tick it sent, whichever arrived first (LOOP-4)', () => {
  // The bridge announces the first tick before it answers the line.
  const ticked = heard(heard(live(), { event: 'loop.tick', session: 's', data: { number: 1, tick: 1, quiet: 0 } }), { event: 'turn.started', session: 's', data: { turn: 2 } })
  const entries = transcript.loopAnswered(ticked.entries, started(1))
  assert.deepEqual(entries.map((entry) => entry.kind), ['user', 'narration', 'loop-tick', 'turn-start'])
  assert.equal(entries[1].text, 'Loop 1 started: every 10s.')
  // An answer that started nothing, or one whose tick has not arrived, is appended.
  const listed = transcript.loopAnswered(ticked.entries, { lines: ['Nothing is repeating.'], refused: false, number: null, pace: null, adjusted: null })
  assert.equal(listed.at(-1).text, 'Nothing is repeating.')
  assert.equal(transcript.loopAnswered(live().entries, started(1)).at(-1).kind, 'narration')
})

test('a held interval is said in the answer row, and a refusal is marked (LOOP-7, LOOP-16)', () => {
  const raised = transcript.loopAnswer(started(1, ['The interval was raised to 1m.', 'Loop 1 started: every 1m.'], { held: 'raised', every: '1m' }))
  assert.equal(raised.text, 'The interval was raised to 1m.\nLoop 1 started: every 1m.')
  assert.equal(raised.refused, undefined)
  const full = transcript.loopAnswer({ lines: ['Five loops are live.'], refused: true, number: null, pace: null, adjusted: null })
  assert.equal(full.refused, true)
  // Markup in a line is kept as its characters: a narration is drawn as text.
  const markup = '<img src=x onerror=alert(1)>'
  assert.equal(transcript.loopAnswer({ lines: [markup], refused: false, number: null, pace: null, adjusted: null }).text, markup)
})

test('the composer asks for the command only from the typed line, and never sends it as a prompt', () => {
  const turn = readFileSync('src/renderer/use-turn.ts', 'utf8')
  assert.doesNotMatch(turn, /loopArgument\(/)
  const staged = readFileSync('src/renderer/staging.ts', 'utf8')
  assert.equal([...staged.matchAll(/loopArgument\(/g)].length, 1)
  // Each of the three places the box sends a line from hands a `/loop` exit to `loopCommand`, after
  // taking what it names and clearing the box.
  const sites = [...turn.matchAll(/if \(exit\.to === 'loop'[^{]*\{(.*?)return\s*\}/gs)]
  assert.equal(sites.length, 3, 'submit, the interject key and the queue key')
  for (const [, site] of sites) {
    assert.match(site, /setDraft\(''\)/)
    assert.match(site, /setLive\(\(old\) => old \? \{ \.\.\.old, attachments: \[\], staging: sent\(old\.staging\) \} : old\)/)
    assert.match(site, /void loopCommand\(.*, exit\.argument, exit\.line, exit\.staged\)$/m)
  }
  const body = turn.slice(turn.indexOf('const loopCommand = '))
  assert.match(body, /call<LoopAnswer>\('loop\.command', \{ session: handle, argument, \.\.\.markersOf\(staged\) \}\)/)
  assert.match(body, /entries: t\.loopAnswered\(old\.entries, answer\)/)
})

test('a start the bridge refuses while a turn runs waits in the queue, and is asked again from it (LOOP-6, CMD-8)', () => {
  const turn = readFileSync('src/renderer/use-turn.ts', 'utf8')
  const body = turn.slice(turn.indexOf('const loopCommand = '), turn.indexOf('const askAside = '))
  // The window queues on the bridge's refusal, never on a reading of the line.
  assert.match(body, /error instanceof RequestFailed && error\.code === 'turn_in_flight' && !queuedAlready\) \{\s*updateSession\(handle, \(old\) => old \? \{ \.\.\.old, queued: \[\.\.\.\(old\.queued \?\? \[\]\), \{ prompt: line, attachments: \[\], staged, loop: argument \}\] \} : old\)\s*return/)
  assert.doesNotMatch(body, /startsWith\(|stop/)
  assert.equal([...turn.matchAll(/loop: argument/g)].length, 2, 'the bridge refusal and a line behind a waiting loop line')
  // Leaving the queue, the line goes back to `loop.command`, not to `send`.
  assert.match(turn, /message\.loop !== undefined\s*\?\s*loopCommand\(handle, message\.loop, message\.prompt, message\.staged, true\)/)
})

test('a /loop stop typed behind a waiting /loop line waits behind it, and is not sent to the bridge first (LOOP-11, CMD-8)', () => {
  const turn = readFileSync('src/renderer/use-turn.ts', 'utf8')
  const body = turn.slice(turn.indexOf('const loopCommand = '))
  const read = body.indexOf('item.loop !== undefined')
  const called = body.indexOf("call<LoopAnswer>('loop.command'")
  assert.ok(read > 0 && called > read, 'the queue is read before loop.command is called')
  assert.match(body.slice(0, called), /!queuedAlready && openedLives\.current\.get\(handle\)\?\.queued\?\.some\(\(item\) => item\.loop !== undefined\)/)
  assert.match(body.slice(0, called), /if \(waiting\) \{.*?queued: \[\.\.\.\(old\.queued \?\? \[\]\), \{ prompt: line, attachments: \[\], staged, loop: argument \}\].*?return\s*\}/s)
})

test('the tick row is drawn as an automatic turn, in the transcript', () => {
  const drawn = readFileSync('src/renderer/components/Transcript.tsx', 'utf8')
  assert.match(drawn, /case 'loop-tick':\s*return <div className="watch-turn"><Icon name="clock" \/><strong>\{entry\.text\}<\/strong>/)
})

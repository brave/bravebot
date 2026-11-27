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
  assert.deepEqual(staging.exitOf(undefined, '/loop 10s say hi'), { to: 'loop', kind: 'loop', argument: '10s say hi', line: '/loop 10s say hi', staged: [] })
  assert.deepEqual(staging.exitOf(undefined, '/loop'), { to: 'loop', kind: 'loop', argument: '', line: '/loop', staged: [] })
  assert.equal(staging.exitOf(undefined, '/looping').to, 'prompt')
  // A `/schedule` line is still the schedule exit.
  assert.equal(staging.exitOf(undefined, '/schedule list').to, 'schedule')
})

test('only /after, alone or with an argument, is the after command, and it takes the loop exit (LOOP-18)', () => {
  assert.equal(shared.afterArgument('/after'), '')
  assert.equal(shared.afterArgument('  /after 5m check the deploy  '), '5m check the deploy')
  for (const prompt of ['/afterwards', '/after-5m x', 'please /after 5m x', 'after 5m x', '/loop 5m x', '']) {
    assert.equal(shared.afterArgument(prompt), null, prompt)
  }
  assert.deepEqual(staging.exitOf(undefined, '/after 5m /status'), { to: 'loop', kind: 'after', argument: '5m /status', line: '/after 5m /status', staged: [] })
  assert.deepEqual(staging.exitOf(undefined, '/after'), { to: 'loop', kind: 'after', argument: '', line: '/after', staged: [] })
  assert.equal(staging.exitOf(undefined, '/afterwards check').to, 'prompt')
  assert.equal(staging.exitOf(undefined, '/loop 5m x').kind, 'loop')
})

test('a /after line goes to after.command, waits only behind a waiting loop line, and leaves the queue as /after (LOOP-18, CMD-8)', () => {
  const turn = readFileSync('src/renderer/use-turn.ts', 'utf8')
  const body = turn.slice(turn.indexOf('const loopCommand = '), turn.indexOf('const askAside = '))
  // The method is named by the exit's kind, and by nothing read off the line here.
  assert.match(body, /call<LoopAnswer>\(`\$\{kind\}\.command`, \{ session: handle, argument, \.\.\.markersOf\(staged\) \}\)/)
  assert.equal([...body.matchAll(/loop: argument, loopKind: kind/g)].length, 2, 'both places a line is queued keep its kind')
  assert.match(turn, /loopCommand\(handle, message\.loopKind \?\? 'loop', message\.loop, message\.prompt, message\.staged, true\)/)
  // The window's main process lets the line through, with its markers and nothing else.
  const index = readFileSync('src/main/index.ts', 'utf8')
  assert.match(index, /'after\.command',/)
  const sanitiser = readFileSync('src/main/sanitise.ts', 'utf8')
  assert.match(sanitiser, /method === 'loop\.command' \|\| method === 'after\.command'\) \{/)
})

test('the answer to /after is drawn as its own row, held lines first, and its one tick is one row (LOOP-18)', () => {
  const answer = { lines: ['The delay was raised to 5s.', 'Loop 1 will send its line in 5s.'], refused: false, number: 1, pace: 'once', adjusted: { held: 'raised', every: '5s' } }
  const entries = transcript.loopAnswered(live().entries, answer)
  assert.equal(entries.at(-1).kind, 'narration')
  assert.equal(entries.at(-1).text, 'The delay was raised to 5s.\nLoop 1 will send its line in 5s.')
  const unreadable = transcript.loopAnswer({ lines: ['/after needs a delay and a line.'], refused: true, number: null, pace: null, adjusted: null })
  assert.equal(unreadable.refused, true)
  const ticked = heard(live(), { event: 'loop.tick', session: 's', data: { number: 2, tick: 1, quiet: 0, once: true, prompt: 'say hi' } })
  assert.equal(ticked.entries.at(-1).text, 'Loop 2 · sent once · say hi')
  // Its ending draws no second row: the tick row already said it was sent once.
  const sent = heard(ticked, { event: 'loop.ended', session: 's', data: { number: 2, reason: 'sent' } })
  assert.equal(sent.entries.length, ticked.entries.length)
})

test('a tick row carries its loop number, its own, its line and the quiet ticks only where there are any (LOOP-13)', () => {
  const first = heard(live(), { event: 'loop.tick', session: 's', data: { number: 2, tick: 1, quiet: 0, once: false, prompt: 'say hi' } })
  const row = first.entries.at(-1)
  assert.equal(row.kind, 'loop-tick')
  assert.equal(row.number, 2)
  assert.equal(row.head, 'Loop 2, tick 1')
  assert.equal(row.line, 'say hi')
  assert.equal(row.quiet, undefined)
  assert.equal(row.text, 'Loop 2, tick 1 · say hi')
  assert.equal(first.running, false, 'the turn is started by the bridge, and says so with turn.started')
  assert.equal(transcript.loopTicked({ number: 2, tick: 5, quiet: 3, once: false, prompt: 'say hi' }).text, 'Loop 2, tick 5 · say hi · 3 quiet')
  // The tick's turn is drawn below it, as a watch's is.
  const turned = heard(first, { event: 'turn.started', session: 's', data: { turn: 4 } })
  assert.deepEqual(turned.entries.slice(-2).map((entry) => entry.kind), ['loop-tick', 'turn-start'])
})

test('an ending says which loop ended, or how many a stop ended (LOOP-11)', () => {
  const one = heard(live(), { event: 'loop.ended', session: 's', data: { number: 3, reason: 'finished' } })
  assert.equal(one.entries.at(-1).kind, 'narration')
  assert.match(one.entries.at(-1).text, /^Loop 3 ended: /)
  assert.equal(transcript.loopEnded({ reason: 'stopped', count: 2 }).text, '2 loops stopped.')
  assert.equal(transcript.loopEnded({ reason: 'stopped', count: 1 }).text, 'The loop stopped.')
  assert.equal(transcript.loopEnded({ number: 1, reason: 'stopped' }).text, 'Loop 1 stopped.')
  assert.match(transcript.loopEnded({ number: 1, reason: 'aged' }).text, /^Loop 1 ended: .*\.$/)
  assert.match(transcript.loopEnded({ number: 1, reason: 'failed', message: 'No model.' }).text, /sent\. No model\.$/)
  const armed = heard(live(), { event: 'loop.armed', session: 's', data: { number: 1, after: '20m' } })
  assert.match(armed.entries.at(-1).text, /^Loop 1 repeats: .*20m\. \/loop stop 1 ends it, and so does Stop or closing the chat\.$/)
})

test('a queued line answered while the tick ahead of it has not replied goes above that tick (LOOP-13)', () => {
  const ticked = heard(heard(live(), { event: 'loop.tick', session: 's', data: { number: 2, tick: 1, quiet: 0, once: false, prompt: 'x' } }), { event: 'turn.started', session: 's', data: { turn: 2 } })
  const refusal = { lines: ['Two loops are running.'], refused: true, number: null, pace: null, adjusted: null }
  const queued = transcript.loopAnswered(ticked.entries, refusal, true)
  assert.deepEqual(queued.map((entry) => entry.kind), ['user', 'narration', 'loop-tick', 'turn-start'])
  // Typed while the tick runs, or once it has replied, the answer is appended.
  assert.equal(transcript.loopAnswered(ticked.entries, refusal).at(-1).kind, 'narration')
  const replied = [...ticked.entries, transcript.replied('Hi.')]
  assert.equal(transcript.loopAnswered(replied, refusal, true).at(-1).kind, 'narration')
})

test('the answer to a start is drawn before the first tick it sent, whichever arrived first (LOOP-4)', () => {
  // The bridge announces the first tick before it answers the line.
  const ticked = heard(heard(live(), { event: 'loop.tick', session: 's', data: { number: 1, tick: 1, quiet: 0, once: false, prompt: 'x' } }), { event: 'turn.started', session: 's', data: { turn: 2 } })
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
  assert.match(body, /call<LoopAnswer>\(`\$\{kind\}\.command`, \{ session: handle, argument, \.\.\.markersOf\(staged\) \}\)/)
  assert.match(body, /entries: t\.loopAnswered\(old\.entries, answer, queuedAlready\)/)
  // The row beside the composer reads the loops again at once (LOOP-13).
  assert.match(body, /window\.dispatchEvent\(new CustomEvent\(LOOPS_CHANGED, \{ detail: handle \}\)\)/)
})

test('a start the bridge refuses while a turn runs waits in the queue, and is asked again from it (LOOP-6, CMD-8)', () => {
  const turn = readFileSync('src/renderer/use-turn.ts', 'utf8')
  const body = turn.slice(turn.indexOf('const loopCommand = '), turn.indexOf('const askAside = '))
  // The window queues on the bridge's refusal, never on a reading of the line.
  assert.match(body, /error instanceof RequestFailed && error\.code === 'turn_in_flight' && !queuedAlready\) \{\s*updateSession\(handle, \(old\) => old \? \{ \.\.\.old, queued: \[\.\.\.\(old\.queued \?\? \[\]\), \{ prompt: line, attachments: \[\], staged, loop: argument, loopKind: kind \}\] \} : old\)\s*return/)
  assert.doesNotMatch(body, /startsWith\(|stop/)
  assert.equal([...turn.matchAll(/loop: argument/g)].length, 2, 'the bridge refusal and a line behind a waiting loop line')
  // Leaving the queue, the line goes back to `loop.command`, not to `send`.
  assert.match(turn, /message\.loop !== undefined\s*\?\s*loopCommand\(handle, message\.loopKind \?\? 'loop', message\.loop, message\.prompt, message\.staged, true\)/)
})

test('a /loop stop typed behind a waiting /loop line waits behind it, and is not sent to the bridge first (LOOP-11, CMD-8)', () => {
  const turn = readFileSync('src/renderer/use-turn.ts', 'utf8')
  const body = turn.slice(turn.indexOf('const loopCommand = '))
  const read = body.indexOf('item.loop !== undefined')
  const called = body.indexOf('call<LoopAnswer>(`${kind}.command`')
  assert.ok(read > 0 && called > read, 'the queue is read before loop.command is called')
  assert.match(body.slice(0, called), /!queuedAlready && openedLives\.current\.get\(handle\)\?\.queued\?\.some\(\(item\) => item\.loop !== undefined\)/)
  assert.match(body.slice(0, called), /if \(waiting\) \{.*?queued: \[\.\.\.\(old\.queued \?\? \[\]\), \{ prompt: line, attachments: \[\], staged, loop: argument, loopKind: kind \}\].*?return\s*\}/s)
})

test('the tick row is drawn as an automatic turn, its line cut to one line with the whole of it in a tooltip', () => {
  const drawn = readFileSync('src/renderer/components/Transcript.tsx', 'utf8')
  const row = drawn.slice(drawn.indexOf("case 'loop-tick':"), drawn.indexOf("case 'error':"))
  assert.match(row, /className="watch-turn loop-turn"/)
  assert.match(row, /<strong>\{entry\.head\}<\/strong>/)
  assert.match(row, /<span className="loop-turn-prompt" data-tooltip=\{entry\.line\}>\{entry\.line\}<\/span>/)
  assert.doesNotMatch(row, /sent again/)
  const css = readFileSync('src/renderer/styles/activity.css', 'utf8')
  assert.match(css, /\.loop-turn-prompt \{[^}]*text-overflow: ellipsis[^}]*color: var\(--ink-secondary\)/)
})

test('a refused narration has a warning glyph in the error colour and its text in the error ink', () => {
  const drawn = readFileSync('src/renderer/components/Transcript.tsx', 'utf8')
  assert.match(drawn, /<Icon name=\{entry\.refused \? 'warning-triangle-outline' : 'file-text'\} \/>/)
  const css = readFileSync('src/renderer/styles/activity.css', 'utf8')
  assert.match(css, /\.narration\.refused \{ color: var\(--status-error-ink\); --leo-icon-color: var\(--status-error\); \}/)
})

test('a waiting /loop or /after line is a command in the queue, so the queue does not call it a turn', () => {
  const composer = readFileSync('src/renderer/components/Composer.tsx', 'utf8')
  // Both kinds wait with `loop` set, so the one test covers both.
  assert.match(composer, /command: [^\n]*\|\| message\.loop !== undefined,/)
})

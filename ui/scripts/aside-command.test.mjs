// `/btw <question>` and `/recap` typed into the composer (CMD-1, CMD-8, CMD-16), as the desktop
// window handles them.
//
// The bridge asks the session's model over a copy of the exchange (crates/ui-bridge/src/aside.rs)
// and never writes to the conversation. What the window owns is which lines go there at all, that
// the answer is a row to read and never part of what a turn sends, that a /btw goes out while a turn
// runs, and that a /recap the bridge refuses mid-turn waits for the turn. The faults these reject
// are a longer word taken for the command, an answer drawn as a user bubble or sent as a prompt, a
// /btw held behind the queue, and a mid-turn /recap that is lost or sent as a prompt.
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

const shared = load("export * from './src/shared/aside'")
const events = load("export { apply } from './src/renderer/apply'")
const transcript = load("export * from './src/renderer/transcript'")

const live = (fields = {}) => ({
  entries: [{ kind: 'user', id: 'u0', text: 'opening prompt' }],
  turns: {}, queued: [], interjected: [], running: false, queuePaused: false,
  summary: { id: null, title: 't', project: '/p', branch: null, directory: '/p' },
  archived: 0, awaitingOrdinal: null, staging: { made: 0, staged: [] }, agents: [],
  ...fields,
})
const heard = (before, message) => {
  let after = before
  events.apply(message, (action) => { after = typeof action === 'function' ? action(after) : action }, () => {}, () => {})
  return after
}

test('only the command word, alone or with a question, is /btw', () => {
  assert.equal(shared.btwQuestion('/btw why does it retry'), 'why does it retry')
  assert.equal(shared.btwQuestion('  /btw   why  '), 'why')
  // Alone, it is the command with an empty question, which the bridge refuses with its sentence.
  assert.equal(shared.btwQuestion('/btw'), '')
  for (const prompt of ['/btwx', '/btwx why', 'say /btw why', 'btw why', '/recap', '']) {
    assert.equal(shared.btwQuestion(prompt), null, prompt)
  }
})

test('only the bare word is /recap', () => {
  assert.equal(shared.isRecap('/recap'), true)
  assert.equal(shared.isRecap('  /recap \n'), true)
  for (const prompt of ['/recap now', '/recaps', 'please /recap', 'recap', '/btw /recap', '']) {
    assert.equal(shared.isRecap(prompt), false, prompt)
  }
})

test('an answer is a narration row and an aside, never a user bubble or a turn', () => {
  const before = live()
  const after = heard(before, { event: 'aside.done', session: 's', data: { status: 'answered', question: 'why', recap: false, answer: 'It retries twice.', kept: true } })
  const row = after.entries.at(-1)
  assert.equal(row.kind, 'narration')
  assert.match(row.text, /^\/btw why\nIt retries twice\.$/)
  assert.equal(row.command, '/btw')
  assert.equal(after.entries.filter((entry) => entry.kind === 'user').length, 1, 'no prompt is drawn')
  assert.equal(after.awaitingOrdinal, null, 'no prompt waits for an ordinal')
  assert.equal(after.running, false, 'an answer starts no turn')
  assert.deepEqual(after.queued, [], 'nothing is queued to be sent')
  assert.deepEqual(after.asides, [{ question: 'why', answer: 'It retries twice.', kept: true }])
})

test('an answer that comes in while a turn runs adds one line and leaves the turn and its queue alone (CMD-8)', () => {
  const queued = [{ prompt: 'next prompt', attachments: [], staged: [] }, { prompt: '/recap', attachments: [], staged: [], aside: 'recap' }]
  const before = live({ running: true, awaitingOrdinal: 3, queued, entries: [...live().entries, transcript.asideOut('why')] })
  const after = heard(before, { event: 'aside.done', session: 's', data: { status: 'answered', question: 'why', recap: false, answer: 'It retries twice.', kept: true } })
  assert.equal(after.running, true, 'the turn still runs')
  assert.equal(after.awaitingOrdinal, 3, 'the prompt still waits for its ordinal')
  assert.equal(after.queued, before.queued, 'the queue is the same queue')
  assert.equal(after.entries.length, before.entries.length + 1, 'one line is added')
  assert.deepEqual(after.entries.slice(0, -1), before.entries, 'no line before it changes')
  assert.equal(after.entries.at(-1).kind, 'narration')
  for (const status of ['stopped', 'failed']) {
    const settled = heard(before, { event: 'aside.done', session: 's', data: { status, question: 'why', recap: false, message: 'the service refused' } })
    assert.equal(settled.running, true, status)
    assert.equal(settled.awaitingOrdinal, 3, status)
    assert.equal(settled.queued, before.queued, status)
    assert.equal(settled.entries.length, before.entries.length + 1, status)
  }
})

test('a /btw asked beside a turn says it is out and names the Asides panel', () => {
  const row = transcript.asideOut('why is it slow')
  assert.equal(row.kind, 'narration')
  assert.equal(row.command, '/btw')
  assert.match(row.text, /^\/btw why is it slow: asked beside the turn\./)
  assert.match(row.text, /Asides panel/)
  const turn = readFileSync('src/renderer/use-turn.ts', 'utf8')
  const body = turn.slice(turn.indexOf('const askAside = '), turn.indexOf('const typedAside = '))
  // Only an answer of besideTurn:true draws it; at rest the answer row is the only line.
  assert.match(body, /if \(started\.besideTurn\) updateSession\(handle, \(old\) => old \? \{ \.\.\.old, entries: \[\.\.\.old\.entries, t\.asideOut\(question\)\] \} : old\)/)
})

test("the bridge's refusal of a /btw is drawn as an error note in its own words", () => {
  const turn = readFileSync('src/renderer/use-turn.ts', 'utf8')
  const body = turn.slice(turn.indexOf('const askAside = '), turn.indexOf('const typedAside = '))
  // A second question while one is out, and a question before the turn has sent anything, are
  // refusals the bridge words (crates/ui-bridge/src/aside.rs); the window shows the sentence as is.
  assert.match(body, /entries: \[\.\.\.old\.entries, t\.errored\(error instanceof RequestFailed \? error\.reason : String\(error\)\)\]/)
  const note = transcript.errored('a question is already out; wait for its answer')
  assert.equal(note.kind, 'error')
  assert.equal(note.text, 'a question is already out; wait for its answer')
})

test('a recap is labelled as one, and an answer the record does not keep says so (WATCH-20)', () => {
  const after = heard(live(), { event: 'aside.done', session: 's', data: { status: 'answered', question: '/recap', recap: true, answer: 'We are adding retries.', kept: false } })
  const row = after.entries.at(-1)
  assert.equal(row.kind, 'narration')
  assert.match(row.text, /^\/recap\nWe are adding retries\.\n/)
  assert.match(row.text, /Not written down/)
  assert.deepEqual(after.asides, [{ question: '/recap', answer: 'We are adding retries.', kept: false }])
})

test('a stopped or failed question is a row and not an aside', () => {
  const stopped = heard(live(), { event: 'aside.done', session: 's', data: { status: 'stopped', question: 'why', recap: false } })
  assert.equal(stopped.entries.at(-1).kind, 'narration')
  assert.equal(stopped.asides, undefined)
  const failed = transcript.asideAnswered({ status: 'failed', question: 'why', recap: false, message: 'the service refused' })
  assert.equal(failed.kind, 'narration')
  assert.equal(failed.refused, true)
  assert.match(failed.text, /the service refused/)
})

test('the answer is plain text, and the row kind is one no turn sends', () => {
  const markup = '<img src=x onerror=alert(1)>'
  const row = transcript.asideAnswered({ status: 'answered', question: 'q', recap: false, answer: markup, kept: true })
  assert.equal(row.text, `/btw q\n${markup}`)
  // Only the user's own rows and the lines sent into a turn are prompts; nothing in use-turn.ts
  // sends a narration, and an aside's answer is never passed to `send` or `turn.send`.
  const turn = readFileSync('src/renderer/use-turn.ts', 'utf8')
  const body = turn.slice(turn.indexOf('const askAside = '), turn.indexOf('const planRun = '))
  assert.doesNotMatch(body, /turn\.send|turn\.interject|\bsend\(|interject\(/)
})

test('the composer asks for the command from the typed line in each place it sends one, before send or interject', () => {
  const turn = readFileSync('src/renderer/use-turn.ts', 'utf8')
  for (const name of ['const queueDraft = ', 'const submit = ', 'const interjectDraft = ']) {
    const start = turn.indexOf(name)
    const body = turn.slice(start, turn.indexOf('\n  }', start))
    const asked = body.indexOf('askAside(')
    assert.ok(asked > 0, `${name} asks the aside`)
    for (const later of ['send(', 'interject(', 'route(', 'queued:']) {
      const at = body.indexOf(later)
      assert.ok(at < 0 || at > asked, `${name} asks the aside before ${later}`)
    }
  }
  // The command is read in one place, `exitOf`, from the line with its folded pastes put back.
  assert.equal([...turn.matchAll(/exitOf\(/g)].length, 3, 'submit, the interject key and the queue key')
  assert.doesNotMatch(turn, /btwQuestion\(|isRecap\(/)
  const staging = readFileSync('src/renderer/staging.ts', 'utf8')
  const exit = staging.slice(staging.indexOf('export function exitOf('))
  assert.match(exit, /const \{ words: line, staged \} = leaving\(staging, draft\)[\s\S]*btwQuestion\(line\)[\s\S]*isRecap\(line\)/)
})

test('a /btw goes out at once while a turn runs and lines wait, and a mid-turn /recap waits (CMD-8, CMD-16)', () => {
  const turn = readFileSync('src/renderer/use-turn.ts', 'utf8')
  const body = turn.slice(turn.indexOf('const askAside = '), turn.indexOf('const planRun = '))
  // Nothing reads the queue or the running flag before asking: the bridge decides.
  assert.doesNotMatch(body.slice(0, body.indexOf("call<AsideStarted>('session.btw'")), /\.queued|\.running/)
  // Only a recap the bridge refused with `turn_in_flight` is queued, marked as one, and never as a prompt.
  assert.match(body, /kind === 'recap' && error instanceof RequestFailed && error\.code === 'turn_in_flight' && !queuedAlready/)
  assert.match(body, /queued: \[\.\.\.\(old\.queued \?\? \[\]\), \{ prompt: line, attachments: \[\], staged: \[\], aside: 'recap' \}\]/)
  assert.equal([...turn.matchAll(/aside: 'recap'/g)].length, 1, 'nothing else makes a queued recap')
  // The queue asks a waiting recap again rather than sending it as a prompt.
  assert.match(turn, /message\.aside === 'recap'\s*\?\s*askAside\(handle, 'recap', '', message\.prompt, \[\], true\)\s*:\s*message\.loop !== undefined/)
  // The line typed into a running turn reaches askAside before the interject key routes it to the queue.
  const interjecting = turn.slice(turn.indexOf('const interjectDraft = '))
  assert.ok(interjecting.indexOf('askAside(') < interjecting.indexOf("route(live"), 'a /btw is not queued behind the turn')
})

test('a /btw sends what was staged on its line with the request, and a recap sends none (CMD-5)', () => {
  const turn = readFileSync('src/renderer/use-turn.ts', 'utf8')
  const body = turn.slice(turn.indexOf('const askAside = '), turn.indexOf('const planRun = '))
  // The items the line still names, found on the folded draft by `exitOf`, used for a /btw only.
  assert.match(body, /const staged = kind === 'btw' \? onLine : \[\]/)
  assert.equal([...turn.matchAll(/askAside\(\w+(?:\.current)?, exit\.kind, exit\.question, exit\.line, exit\.staged\)/g)].length, 3, 'submit, the interject key and the queue key')
  // Pasted pictures and dropped pictures or PDFs go as grant ids, and a text file as its marker and name.
  assert.match(body, /pastes: grantsOf\(staged\)\.pastes/)
  assert.match(body, /drops: staged\.flatMap\(\(item\) => item\.via === 'drop' && item\.file\.kind !== 'text' \? \[item\.file\.id\] : \[\]\)/)
  assert.match(body, /files: markersOf\(staged\.filter\(\(item\) => item\.via === 'drop' && item\.file\.kind === 'text'\)\)\.files/)
  assert.match(body, /call<AsideStarted>\('session\.btw', \{ session: handle, question, model, \.\.\.carried \}\)/)
  assert.match(body, /call<AsideStarted>\('session\.recap', \{ session: handle, model \}\)/)
  // Sent items leave the composer, and a refused line goes back to it with them.
  assert.match(body, /if \(staged\.length > 0\) updateSession\(handle, \(old\) => old \? \{ \.\.\.old, staging: sent\(old\.staging\) \} : old\)/)
  assert.match(body, /if \(staged\.length > 0 && handleRef\.current === handle\) setDraft\(line\)/)
  assert.doesNotMatch(body, /cannot carry files or pictures yet/, 'staged items are no longer refused')
})

// The Asides panel, drawn through `react-dom/server` as notes.test.mjs draws the Notes panel. The
// file tree reads the disk, so it is left out, and a Leo button is drawn as a plain one.
const React = require('react')
const { renderToStaticMarkup } = require('react-dom/server')
const withoutTree = {
  name: 'without-tree',
  setup(built) {
    built.onResolve({ filter: /^\.\/FileTree$/ }, () => ({ path: 'tree', namespace: 'no-tree' }))
    built.onLoad({ filter: /.*/, namespace: 'no-tree' }, () => ({ loader: 'js', contents: 'export const FileTree = () => null' }))
  },
}
let column
const context = async () => {
  const { loadComponents } = await import('./render-support.mjs')
  return (column ??= loadComponents({ contents: "export { Context } from './src/renderer/components/Context.tsx'" }, [withoutTree]))
}
const asidesPanel = async (asides) => {
  const { Context } = await context()
  const inspected = { handle: 's-1', summary: { directory: '/p' }, entries: [], todos: [], quarantine: [], phase: null, tokens: 0, running: false, asides }
  const markup = renderToStaticMarkup(React.createElement(Context, { live: inspected }))
  const start = markup.indexOf('id="panel-asides"')
  assert.ok(start > 0, 'the column has an Asides panel')
  return markup.slice(start, markup.indexOf('</section>', start))
}

test('the Asides panel lists each question and answer as plain text, and counts them', async () => {
  const panel = await asidesPanel([
    { question: 'why <b>retry</b>', answer: 'It retries <img src=x>.', kept: true },
    { question: '/recap', answer: 'We are adding retries.', kept: true },
  ])
  assert.ok(panel.includes('why &lt;b&gt;retry&lt;/b&gt;') && panel.includes('It retries &lt;img src=x&gt;.'), panel)
  assert.ok(panel.includes('/recap') && panel.includes('We are adding retries.'), panel)
  assert.equal(panel.match(/<li\b/g).length, 2)
  assert.ok(panel.includes(', 2 items'), panel)
  assert.doesNotMatch(panel, /not written down|did not keep/, 'a kept answer carries no note')
})

test('the Asides panel says when an answer is not written down, and when the record did not keep one (WATCH-20)', async () => {
  const unkept = await asidesPanel([{ question: 'why', answer: 'Shown, not kept.', kept: false }])
  assert.ok(unkept.includes('Shown, not kept.'), unkept)
  assert.match(unkept, /This answer is not written down in the record\./)
  const resumed = await asidesPanel([{ question: 'why', answer: null, kept: false }])
  assert.match(resumed, /The record did not keep this answer\./)
  assert.doesNotMatch(resumed, /not written down/, 'one note for a resumed aside')
  const none = await asidesPanel([])
  assert.match(none, /No asides\./)
})

test('a session opened or resumed seeds its asides from the bridge answer', () => {
  const app = readFileSync('src/renderer/App.tsx', 'utf8')
  assert.match(app, /asides: opened\.asides \?\? \[\]/)
})

test('the bridge methods are ones the window may send', () => {
  const index = readFileSync('src/main/index.ts', 'utf8')
  assert.match(index, /'session\.btw',\s*'session\.recap',\s*'session\.aside\.cancel',/)
})

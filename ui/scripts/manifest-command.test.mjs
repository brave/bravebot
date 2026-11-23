// `/manifest` typed into the composer (CMD-1, MANIFEST-9), as the desktop window reads it.
//
// The bridge plans and runs the task (crates/ui-bridge/src/manifest.rs). What the window owns is
// which lines start a run at all. The faults these reject are a longer word taken for the command,
// and a line that only mentions the command being taken for it.
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

const shared = load("export * from './src/shared/manifest'")

test('the command word with a task gives the trimmed task', () => {
  assert.equal(shared.manifestTask('/manifest t'), 't')
  assert.equal(shared.manifestTask('/manifest fix the docs'), 'fix the docs')
  assert.equal(shared.manifestTask('  /manifest   fix the docs  '), 'fix the docs')
})

test('the command word alone gives an empty task', () => {
  assert.equal(shared.manifestTask('/manifest'), '')
  assert.equal(shared.manifestTask('  /manifest  '), '')
})

test('any other line is not the command', () => {
  assert.equal(shared.manifestTask('/manifestx'), null)
  assert.equal(shared.manifestTask('/manifestx t'), null)
  assert.equal(shared.manifestTask('say /manifest'), null)
  assert.equal(shared.manifestTask('say /manifest x'), null)
  assert.equal(shared.manifestTask('manifest x'), null)
  assert.equal(shared.manifestTask(''), null)
})

// The dispatch, run with a stand-in for the window: what is asked of the bridge, and what is drawn.
const running = load("export * from './src/renderer/plan-run'; export { RequestFailed } from './src/renderer/rewind'")
const events = load("export { apply } from './src/renderer/apply'")

const live = () => ({
  handle: 's', entries: [{ kind: 'user', id: 'u0', text: 'opening prompt' }],
  turns: {}, queued: [], interjected: [], running: false, queuePaused: false, attachments: [], model: 'm1',
  summary: { id: null, title: 't', project: '/p', branch: null, directory: '/p' },
  archived: 0, awaitingOrdinal: null, staging: { made: 0, staged: [] }, agents: [],
})
const heard = (before, message) => {
  let after = before
  events.apply(message, (action) => { after = typeof action === 'function' ? action(after) : action }, () => {}, () => {})
  return after
}
/** A window holding one chat, `s`, whose bridge answers each call with `answer(method)`. */
const windowWith = (state, answer = () => ({})) => {
  const seen = { state, calls: [], drafts: [], problems: [], drawnBeforeCall: null }
  seen.on = {
    call: async (method, params) => {
      seen.calls.push({ method, params })
      seen.drawnBeforeCall = seen.state.entries.map((entry) => entry.kind)
      return answer(method)
    },
    read: () => seen.state,
    update: (_handle, change) => { seen.state = change(seen.state) },
    putBack: (_handle, line) => seen.drafts.push(line),
    setProblem: (problem) => seen.problems.push(problem),
  }
  return seen
}

test('a typed /manifest t sends one manifest.run with the session, task and model, and no turn.send', async () => {
  const seen = windowWith(live())
  await running.planRun(seen.on, 's', 't', '/manifest t')
  assert.deepEqual(seen.calls, [{ method: 'manifest.run', params: { session: 's', task: 't', model: 'm1' } }])
})

test('the task is drawn as a plan-task row, before the call, and no user row is added', async () => {
  const seen = windowWith(live())
  await running.planRun(seen.on, 's', 't', '/manifest t')
  assert.deepEqual(seen.drawnBeforeCall, ['user', 'plan-task'])
  assert.deepEqual(seen.state.entries.map((entry) => entry.kind), ['user', 'plan-task'])
  assert.equal(seen.state.entries.at(-1).text, 't')
})

test('a turn_in_flight refusal takes the row back off and queues the line with its task', async () => {
  const seen = windowWith(live(), () => { throw new running.RequestFailed('turn_in_flight', 'a turn is running') })
  await running.planRun(seen.on, 's', 't', '/manifest t')
  assert.deepEqual(seen.state.queued, [{ prompt: '/manifest t', attachments: [], staged: [], manifest: 't' }])
  assert.deepEqual(seen.state.entries.map((entry) => entry.kind), ['user'])
})

test('any other refusal takes the row back off and shows the bridge sentence', async () => {
  const seen = windowWith(live(), () => { throw new running.RequestFailed('bad_request', 'the planner refused') })
  await running.planRun(seen.on, 's', 't', '/manifest t')
  assert.deepEqual(seen.state.entries.map((entry) => entry.kind), ['user', 'error'])
  assert.equal(seen.state.entries.at(-1).text, 'the planner refused')
  assert.deepEqual(seen.state.queued, [])
})

test('a line from the queue refused again is shown as an error, not queued a second time', async () => {
  const seen = windowWith(live(), () => { throw new running.RequestFailed('turn_in_flight', 'a turn is running') })
  await running.planRun(seen.on, 's', 't', '/manifest t', true)
  assert.deepEqual(seen.state.queued, [])
  assert.equal(seen.state.entries.at(-1).kind, 'error')
})

test('an empty task adds the terminal note and sends nothing', async () => {
  const seen = windowWith(live())
  await running.planRun(seen.on, 's', '', '/manifest')
  assert.deepEqual(seen.calls, [])
  assert.equal(seen.state.entries.at(-1).kind, 'narration')
  assert.equal(seen.state.entries.at(-1).text, 'a manifest run needs a task to plan')
})

test('paperclip files or staged items on the line put the draft back and send nothing', async () => {
  const clipped = windowWith({ ...live(), attachments: [{ id: 'f1', path: '/p/a.txt' }] })
  await running.planRun(clipped.on, 's', 't', '/manifest t')
  const dropped = windowWith({ ...live(), staging: { made: 1, staged: [{ marker: '[File #1]', via: 'drop', file: { kind: 'text', name: 'a.txt', id: 'd1' } }] } })
  await running.planRun(dropped.on, 's', 't [File #1]', '/manifest t [File #1]')
  for (const [seen, line] of [[clipped, '/manifest t'], [dropped, '/manifest t [File #1]']]) {
    assert.deepEqual(seen.calls, [])
    assert.deepEqual(seen.drafts, [line])
    assert.equal(seen.problems.length, 1)
    assert.match(seen.problems[0], /takes no files/)
    assert.deepEqual(seen.state.entries.map((entry) => entry.kind), ['user'])
  }
})

test('manifest.started then manifest.done leaves the chat not running, with the reply after the task', async () => {
  const seen = windowWith(live())
  await running.planRun(seen.on, 's', 't', '/manifest t')
  const started = heard(seen.state, { event: 'manifest.started', session: 's', data: {} })
  assert.equal(started.running, true)
  const done = heard(started, { event: 'manifest.done', session: 's', data: { reply: 'Listed 3 files.', record: 'r1' } })
  assert.equal(done.running, false)
  assert.deepEqual(done.entries.map((entry) => entry.kind), ['user', 'plan-task', 'plan-reply'])
  assert.equal(done.entries.at(-1).text, 'Listed 3 files.')
})

test('every composer path and the queue start a /manifest line as a run, never as a prompt', () => {
  const turn = readFileSync('src/renderer/use-turn.ts', 'utf8')
  for (const name of ['const queueDraft', 'const submit', 'const interjectDraft']) {
    const body = turn.slice(turn.indexOf(name))
    const branch = body.indexOf('manifestTask(')
    const sent = Math.min(...['send(prompt)', 'interject(handle, line)', 'queued: [...(old.queued'].map((said) => body.indexOf(said)).filter((at) => at >= 0))
    assert.ok(branch > 0 && branch < sent, name)
    assert.match(body.slice(branch, branch + 200), /planRun\(/, name)
  }
  assert.match(turn, /message\.manifest !== undefined\s*\?\s*planRun\(handle, message\.manifest, message\.prompt, true\)/)
})

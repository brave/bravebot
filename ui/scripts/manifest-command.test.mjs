// `/manifest` typed into the composer (CMD-1, MANIFEST-9, DROP-4), as the desktop window reads it.
//
// The bridge plans and runs the task (crates/ui-bridge/src/manifest.rs), and the main process turns
// the window's drop grants into the run's `attachments` (src/main/sanitise.ts). What the window owns
// is which lines start a run at all, and that what was dropped on the line goes with it as grant ids
// and markers. The faults these reject are a longer word taken for the command, a line that only
// mentions the command being taken for it, a paperclip file or a pasted picture sent with the task,
// and a waiting line that loses its drops.
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
  assert.equal(shared.manifestTask('/manifest build [Image #1]'), 'build [Image #1]')
})

test('the command word alone gives an empty task', () => {
  assert.equal(shared.manifestTask('/manifest'), '')
  assert.equal(shared.manifestTask('  /manifest  '), '')
})

test('any other line is not the command', () => {
  assert.equal(shared.manifestTask('/manifestx'), null)
  assert.equal(shared.manifestTask('/manifestx t'), null)
  assert.equal(shared.manifestTask('/manifests x'), null)
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

const picture = { marker: '[Image #1]', via: 'drop', file: { kind: 'image', name: 'shot.png', id: 'g1' } }
const textFile = { marker: '[File #2]', via: 'drop', file: { kind: 'text', name: 'a.txt', id: 'd2' } }
const pasted = { marker: '[Image #3]', via: 'paste', picture: { id: 'p3', thumbnail: '' } }
const withStaged = (...staged) => ({ ...live(), staging: { made: staged.length, staged } })

test('a typed /manifest t sends one manifest.run with the session, task and model, and no turn.send', async () => {
  const seen = windowWith(live())
  await running.planRun(seen.on, 's', 't', '/manifest t')
  assert.deepEqual(seen.calls, [{ method: 'manifest.run', params: { session: 's', task: 't', model: 'm1', drops: [], markers: [], files: [] } }])
})

test('a dropped picture goes as its grant id and a dropped text file as its marker and name, never a path (DROP-4)', async () => {
  const seen = windowWith(withStaged(picture, textFile))
  await running.planRun(seen.on, 's', 'build [Image #1] from [File #2]', '/manifest build [Image #1] from [File #2]')
  assert.deepEqual(seen.calls, [{
    method: 'manifest.run',
    params: { session: 's', task: 'build [Image #1] from [File #2]', model: 'm1', drops: ['g1'], markers: ['[Image #1]'], files: [{ marker: '[File #2]', name: 'a.txt' }] },
  }])
  assert.deepEqual(seen.problems, [])
  assert.deepEqual(seen.state.staging.staged, [], 'what was sent leaves the composer')
})

test('only what the line still names goes with it', async () => {
  const seen = windowWith(withStaged(picture, textFile))
  await running.planRun(seen.on, 's', 'build [Image #1]', '/manifest build [Image #1]')
  assert.deepEqual(seen.calls[0].params.drops, ['g1'])
  assert.deepEqual(seen.calls[0].params.files, [])
})

test('the task is drawn as a plan-task row, before the call, and no user row is added', async () => {
  const seen = windowWith(live())
  await running.planRun(seen.on, 's', 't', '/manifest t')
  assert.deepEqual(seen.drawnBeforeCall, ['user', 'plan-task'])
  assert.deepEqual(seen.state.entries.map((entry) => entry.kind), ['user', 'plan-task'])
  assert.equal(seen.state.entries.at(-1).text, 't')
})

test('a turn_in_flight refusal takes the row back off and queues the line with its task and drops', async () => {
  const seen = windowWith(withStaged(picture), () => { throw new running.RequestFailed('turn_in_flight', 'a turn is running') })
  await running.planRun(seen.on, 's', 't [Image #1]', '/manifest t [Image #1]')
  assert.deepEqual(seen.state.queued, [{ prompt: '/manifest t [Image #1]', attachments: [], staged: [picture], manifest: 't [Image #1]' }])
  assert.deepEqual(seen.state.entries.map((entry) => entry.kind), ['user'])
  assert.deepEqual(seen.state.staging.staged, [], 'the waiting line holds its drops, the composer does not')
})

test('a line from the queue sends the drops it waited with, not what the composer holds now', async () => {
  const seen = windowWith(withStaged(textFile))
  await running.planRun(seen.on, 's', 't [Image #1]', '/manifest t [Image #1]', true, [picture])
  assert.deepEqual(seen.calls[0].params.drops, ['g1'])
  assert.deepEqual(seen.calls[0].params.files, [])
  assert.deepEqual(seen.state.staging.staged, [textFile], 'the next draft keeps its own drops')
})

test('any other refusal takes the row back off and shows the bridge sentence', async () => {
  const seen = windowWith(live(), () => { throw new running.RequestFailed('bad_request', 'the planner refused') })
  await running.planRun(seen.on, 's', 't', '/manifest t')
  assert.deepEqual(seen.state.entries.map((entry) => entry.kind), ['user', 'error'])
  assert.equal(seen.state.entries.at(-1).text, 'the planner refused')
  assert.deepEqual(seen.state.queued, [])
})

test('a refused line with drops on it goes back in the composer with them', async () => {
  const seen = windowWith(withStaged(picture), () => { throw new running.RequestFailed('bad_request', 'the planner refused') })
  await running.planRun(seen.on, 's', 't [Image #1]', '/manifest t [Image #1]')
  assert.deepEqual(seen.drafts, ['/manifest t [Image #1]'])
  assert.deepEqual(seen.state.staging.staged, [picture])
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

test('a paperclip file, a pasted picture or a folded paste on the line puts the draft back and sends nothing', async () => {
  const folded = { marker: '[Pasted text #4 +5 lines]', via: 'text', pasted: { marker: '[Pasted text #4 +5 lines]', text: 'a\nb\nc\nd\ne\nf' } }
  const cases = [
    [{ ...live(), attachments: [{ id: 'f1', path: '/p/a.txt' }] }, '/manifest t', running.NO_PAPERCLIP],
    [withStaged(pasted), '/manifest t [Image #3]', running.NO_PASTED_PICTURE],
    [withStaged(picture, folded), '/manifest t [Image #1] [Pasted text #4 +5 lines]', running.ONLY_DROPS],
  ]
  for (const [state, line, problem] of cases) {
    const seen = windowWith(state)
    await running.planRun(seen.on, 's', shared.manifestTask(line), line)
    assert.deepEqual(seen.calls, [], line)
    assert.deepEqual(seen.drafts, [line])
    assert.deepEqual(seen.problems, [problem])
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

test('the plan card replaces the agent’s narration of the same steps, and the goal stays', async () => {
  const seen = windowWith(live())
  await running.planRun(seen.on, 's', 't', '/manifest t')
  const steps = ['1. [act] write NOTES.md: new file, 1 line']
  let state = heard(seen.state, { event: 'narration', session: 's', data: { text: 'Goal, as understood:\nWrite the notes.' } })
  state = heard(state, { event: 'narration', session: 's', data: { text: `Plan, fixed before anything runs:\n  ${steps[0]}\n` } })
  state = heard(state, { event: 'manifest.request', session: 's', data: { request: 1, task: 't', steps } })
  assert.deepEqual(state.entries.map((entry) => entry.kind), ['user', 'plan-task', 'narration', 'manifest'])
  assert.match(state.entries[2].text, /^Goal, as understood/)

  // A narration whose lines are not the card's steps is kept.
  let other = heard(seen.state, { event: 'narration', session: 's', data: { text: 'Plan, fixed before anything runs:\n  1. [act] write OTHER.md\n' } })
  other = heard(other, { event: 'manifest.request', session: 's', data: { request: 1, task: 't', steps } })
  assert.deepEqual(other.entries.map((entry) => entry.kind), ['user', 'plan-task', 'narration', 'manifest'])
})

const composer = load("export { queuedRow, queueNote } from './src/renderer/components/Composer'")

test('a queued /manifest line is marked as a plan, and the caption does not call a command a turn', () => {
  const prompt = composer.queuedRow({ prompt: 'hello', attachments: [], staged: [] })
  const plan = composer.queuedRow({ prompt: '/manifest t', attachments: [], staged: [], manifest: 't' })
  const schedule = composer.queuedRow({ prompt: '/schedule list', attachments: [], staged: [], schedule: 'list' })
  assert.deepEqual(prompt, { text: 'hello', plan: false, command: false })
  assert.deepEqual(plan, { text: '/manifest t', plan: true, command: true })
  assert.deepEqual(schedule, { text: '/schedule list', plan: false, command: true })
  assert.equal(composer.queueNote([prompt], false), 'Each starts a turn of its own after this one')
  assert.equal(composer.queueNote([prompt, plan], false), 'Each starts after this one')
  assert.equal(composer.queueNote([plan], true), 'Held until you resume')
  const markup = readFileSync('src/renderer/components/Composer.tsx', 'utf8')
  assert.match(markup, /\{plan && <span className="tray-mark plan-task-mark">Plan<\/span>\}/)
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
  // A waiting line starts its run with what it waited with (DROP-4).
  assert.match(turn, /message\.manifest !== undefined\s*\?\s*planRun\(handle, message\.manifest, message\.prompt, true, message\.staged\)/)
})

test('each dropped picture goes with its own marker, and the plan card and the queued row show them (DROP-4)', async () => {
  const second = { marker: '[Image #4]', via: 'drop', file: { kind: 'pdf', name: 'spec.pdf', id: 'g4' } }
  const seen = windowWith(withStaged(second, picture))
  await running.planRun(seen.on, 's', 'compare [Image #1] with [Image #4]', '/manifest compare [Image #1] with [Image #4]')
  // Paired by position, so the bridge names each file where its own marker stands.
  assert.deepEqual(seen.calls[0].params.drops, ['g4', 'g1'])
  assert.deepEqual(seen.calls[0].params.markers, ['[Image #4]', '[Image #1]'])
  const card = heard(seen.state, { event: 'manifest.request', session: 's', data: { request: 1, task: 'compare', steps: [] } }).entries.at(-1)
  assert.deepEqual(card.dropped.map((item) => item.marker), ['[Image #4]', '[Image #1]'])
  const done = heard(seen.state, { event: 'manifest.done', session: 's', data: { reply: 'ok', record: 'r1' } })
  assert.equal(done.planDropped, undefined, 'the next run starts with none')
  const row = composer.queuedRow({ prompt: '/manifest t [Image #1]', attachments: [], staged: [picture], manifest: 't [Image #1]' })
  assert.deepEqual(row.dropped, [{ marker: '[Image #1]', file: picture.file }])
})

test('the bridge method is one the window may send', () => {
  const index = readFileSync('src/main/index.ts', 'utf8')
  assert.match(index, /'manifest\.run',/)
})

// Chat › Plan and Run…, which only writes the start of the line into the composer.
const menu = load("export * from './src/shared/commands'; export { canStart, canSend } from './src/renderer/can-send'")

test('Plan and Run… leaves /manifest in an empty composer, keeps a typed draft as the task, and sends nothing', () => {
  assert.equal(shared.planPrefill(''), '/manifest ')
  assert.equal(shared.planPrefill('  \n'), '/manifest ')
  assert.equal(shared.planPrefill('  list the files '), '/manifest list the files')
  assert.equal(shared.planPrefill('/manifest list the files'), '/manifest list the files')
  assert.equal(shared.planPrefill('/manifest '), '/manifest ')
  // The prefilled line is the command with no task yet, so sending it as it stands gets the
  // terminal's note rather than a run.
  assert.equal(shared.manifestTask(shared.planPrefill('')), '')
  // The handler writes the draft and focuses the composer. It calls nothing on the bridge.
  const app = readFileSync('src/renderer/App.tsx', 'utf8')
  const handler = app.slice(app.indexOf('    plan: () => {'), app.indexOf('    cancel:', app.indexOf('    plan: () => {')))
  assert.match(handler, /setDraft\(planPrefill\(draft\)\)/)
  assert.match(handler, /FOCUS_COMPOSER_EVENT/)
  assert.doesNotMatch(handler, /call\(|submit|send|planRun/)
})

test('Plan and Run… is a Chat item with no shortcut, offered with an empty composer and grey when no session can start', () => {
  const item = menu.command('turn.plan')
  assert.equal(item.label, 'Plan and Run…')
  assert.equal(item.accelerator, undefined)
  const idle = { ...menu.NOTHING_OPEN, hasSession: true, canStart: true }
  assert.equal(menu.isEnabled(item.requires, idle), true, 'an idle session with an empty composer')
  assert.equal(menu.isEnabled(item.requires, menu.NOTHING_OPEN), false, 'no session open')
  assert.equal(menu.isEnabled(item.requires, { ...idle, running: true, canStart: false }), false, 'a turn running')
  const ready = { running: false, askingTrust: null }
  assert.equal(menu.canStart(ready, true), true)
  assert.equal(menu.canSend(ready, true, ''), false, 'Send would be grey here, which is why the item does not use it')
  assert.equal(menu.canStart({ ...ready, running: true }, true), false)
  assert.equal(menu.canStart({ ...ready, askingTrust: '/p' }, true), false)
  assert.equal(menu.canStart(ready, false), false)
  assert.equal(menu.canStart(null, true), false)
  assert.equal(menu.parseWindowState({ ...idle, canStart: undefined }), null, 'a state that leaves it out enables nothing')
  assert.match(readFileSync('src/main/menu.ts', 'utf8'), /item\('turn\.send'\),\s*item\('turn\.plan'\),/)
})

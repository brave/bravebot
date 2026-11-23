// `/plan` typed into the composer (CMD-1, MODE-12), as the desktop window reads the line.
//
// The faults these reject are a longer word or a mid-line mention taken for the command, and the
// typed line, rather than its task, being what a turn would be sent.
import test from 'node:test'
import assert from 'node:assert/strict'
import { buildSync } from 'esbuild'
import { createRequire } from 'node:module'
import { readFileSync } from 'node:fs'

const require = createRequire(import.meta.url)
const load = (contents) => {
  const source = buildSync({ stdin: { contents, resolveDir: process.cwd(), loader: 'ts' }, bundle: true, write: false, platform: 'node', format: 'cjs', external: ['electron'] }).outputFiles[0].text
  const module = { exports: {} }
  new Function('require', 'module', 'exports', source)(require, module, module.exports)
  return module.exports
}

const shared = load("export * from './src/shared/plan'")

test('the command word alone gives an empty task', () => {
  assert.equal(shared.planTask('/plan'), '')
  assert.equal(shared.planTask('  /plan  '), '')
})

test('the command word with a task gives the task alone, trimmed', () => {
  assert.equal(shared.planTask('  /plan   fix the build '), 'fix the build')
  assert.equal(shared.planTask('/plan list the TODOs'), 'list the TODOs')
})

test('a longer word, a sentence that mentions the command, or another command is a prompt', () => {
  for (const prompt of ['/planned', '/planx', 'say /plan x', 'plan x', '/schedule x', '/Plan x', '']) {
    assert.equal(shared.planTask(prompt), null, prompt)
  }
})

// Carrying the line out, against a stubbed `call` and `send` that write one log in the order they
// are reached. The faults these reject are a turn sent before plan mode is set, the typed line sent
// in place of the task, a bare `/plan` starting a turn, a plan line going out while a turn runs, and
// a turn sent after the mode request failed.
// One bundle, so the `RequestFailed` thrown here is the class `carryOutPlan` checks for.
const { carryOutPlan, RequestFailed, failureSummary } = load("export { carryOutPlan } from './src/renderer/plan-line'\nexport { RequestFailed } from './src/renderer/rewind'\nexport { failureSummary } from './src/renderer/failure'")

const harness = (session, { modeFails = false } = {}) => {
  const log = []
  const sessions = new Map([['h', { entries: [], queued: [], permissionMode: 'ask', ...session }]])
  return {
    log,
    session: () => sessions.get('h'),
    planning: {
      call: async (method, params) => {
        log.push({ method, params })
        if (method !== 'session.mode') throw new Error(`unexpected ${method}`)
        if (modeFails) throw new RequestFailed('bad_request', 'the mode could not be set.')
        return { permissionMode: params.mode }
      },
      read: (handle) => sessions.get(handle),
      update: (handle, change) => sessions.set(handle, change(sessions.get(handle))),
      send: async (prompt, target, selectedFiles, staged) => {
        log.push({ method: 'turn.send', params: { prompt, target, selectedFiles, staged } })
      },
    },
  }
}

test('plan mode is set before the turn is sent, and the turn is sent the task alone', async () => {
  const h = harness({ running: false })
  await carryOutPlan(h.planning, 'h', 'list the TODOs', '/plan list the TODOs', [])
  assert.deepEqual(h.log.map((entry) => entry.method), ['session.mode', 'turn.send'])
  assert.deepEqual(h.log[0].params, { session: 'h', mode: 'plan' })
  assert.equal(h.log[1].params.prompt, 'list the TODOs')
  assert.equal(h.log[1].params.target, 'h')
  assert.equal(h.session().permissionMode, 'plan')
})

test('bare /plan sets plan mode and sends nothing', async () => {
  const h = harness({ running: false })
  await carryOutPlan(h.planning, 'h', '', '/plan', [])
  assert.deepEqual(h.log.map((entry) => entry.method), ['session.mode'])
  assert.equal(h.session().permissionMode, 'plan')
})

test('a plan line typed during a turn waits in the queue, and is carried out when it reaches the front', async () => {
  const h = harness({ running: true })
  await carryOutPlan(h.planning, 'h', 'x', '/plan x', [])
  assert.deepEqual(h.log, [], 'nothing is asked of the bridge while the turn runs')
  assert.deepEqual(h.session().queued, [{ prompt: '/plan x', attachments: [], staged: [], plan: 'x' }])
  assert.equal(h.session().permissionMode, 'ask')

  // A second plan line waits behind the first even once the turn has ended.
  h.planning.update('h', (old) => ({ ...old, running: false }))
  await carryOutPlan(h.planning, 'h', 'y', '/plan y', [])
  assert.deepEqual(h.log, [])
  assert.deepEqual(h.session().queued.map((item) => item.plan), ['x', 'y'])

  // The dequeue effect hands the front line back with `queuedAlready`.
  const [front] = h.session().queued
  await carryOutPlan(h.planning, 'h', front.plan, front.prompt, front.staged, true)
  assert.deepEqual(h.log.map((entry) => entry.method), ['session.mode', 'turn.send'])
  assert.equal(h.log[1].params.prompt, 'x')
  assert.deepEqual(h.log[1].params.selectedFiles, [], 'a queued line carries no file chosen with the paperclip')
})

test('a mode the bridge did not set is shown as an error and starts no turn', async () => {
  const h = harness({ running: false }, { modeFails: true })
  await carryOutPlan(h.planning, 'h', 'list the TODOs', '/plan list the TODOs', [])
  assert.deepEqual(h.log.map((entry) => entry.method), ['session.mode'])
  assert.equal(h.session().permissionMode, 'ask')
  assert.equal(h.session().entries.length, 1)
  assert.equal(h.session().entries[0].kind, 'error')
  assert.equal(h.session().entries[0].category, 'plan-mode')
  assert.equal(h.session().entries[0].text, 'the mode could not be set.')
  const card = failureSummary('plan-mode')
  assert.equal(card.title, 'Could not switch to Plan mode')
  assert.equal(card.description, 'Nothing was sent, so try again or choose Plan from the mode menu.')
})

// The composer's three ways out, and the queue, each go through the parser before anything else
// reads the line as a prompt. Read from the source as schedule-command.test.mjs reads it.
const turn = readFileSync(new URL('../src/renderer/use-turn.ts', import.meta.url), 'utf8')
const between = (start, end) => turn.slice(turn.indexOf(start), turn.indexOf(end, turn.indexOf(start)))

test('submit, interject and queue take a plan line before it can become a prompt or an interjection', () => {
  const submit = between('const submit = useCallback', '/** What the composer')
  assert.match(submit, /planTask\(prompt\)[\s\S]*void plan\(handleRef\.current, task, prompt, staged\)[\s\S]*void send\(prompt\)/)
  const interjectDraft = between('const interjectDraft = ', 'return { ')
  assert.ok(interjectDraft.indexOf('planTask(text)') < interjectDraft.indexOf('void interject('))
  assert.match(interjectDraft, /void plan\(handle, task, text, staged\)/)
  const queueDraft = between('const queueDraft = ', '/** Send whatever')
  assert.match(queueDraft, /planTask\(draft\)[\s\S]*plan: task \}/)
  assert.match(turn, /message\.plan !== undefined\s*\? plan\(handle, message\.plan, message\.prompt, message\.staged, true\)/)
  assert.match(turn, /return \{[^}]*\bplan \}/)
})

test('nothing in the command asks for bypass', () => {
  const planLine = readFileSync(new URL('../src/renderer/plan-line.ts', import.meta.url), 'utf8')
  const parser = readFileSync(new URL('../src/shared/plan.ts', import.meta.url), 'utf8')
  assert.doesNotMatch(planLine + parser, /bypass/i)
})

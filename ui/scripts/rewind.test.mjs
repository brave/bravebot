import test from 'node:test'
import assert from 'node:assert/strict'
import { createRequire } from 'node:module'
import { buildSync } from 'esbuild'
const require = createRequire(import.meta.url)
function load(path) {
  const source = buildSync({ entryPoints: [path], bundle: true, write: false, platform: 'node', format: 'cjs' }).outputFiles[0].text
  const module = { exports: {} }
  new Function('require', 'module', 'exports', source)(require, module, module.exports)
  return module.exports
}
const { planRewind, gapWarning, pointForPrompt, undoRow, RequestFailed, rewindFailure } = load('src/renderer/rewind.ts')
const t = load('src/renderer/transcript.ts')
const { CONTEXT, NOTHING_OPEN, isEnabled, parseWindowState, command } = load('src/shared/commands.ts')

const points = [
  { steps: 1, turn: 4, prompt: 3, text: 'fourth', paths: ['src/a.rs', 'src/b.rs'], gaps: [] },
  { steps: 2, turn: 3, prompt: 2, text: 'third', paths: ['src/b.rs', 'README.md'], gaps: ['command'] },
  { steps: 3, turn: 2, prompt: null, text: 'second', paths: ['src/c.rs'], gaps: ['hook', 'command'] },
]

test('a rewind names every file the turns it undoes wrote, each once, and none it leaves alone', () => {
  assert.deepEqual(planRewind(points, 1), { steps: 1, turn: 4, paths: ['src/a.rs', 'src/b.rs'], warnings: [] })
  const two = planRewind(points, 2)
  assert.equal(two.turn, 3)
  assert.deepEqual(two.paths, ['src/a.rs', 'src/b.rs', 'README.md'])
  assert.ok(!two.paths.includes('src/c.rs'), 'a turn further back than the rewind keeps its files')
  assert.deepEqual(planRewind(points, 3).paths, ['src/a.rs', 'src/b.rs', 'README.md', 'src/c.rs'])
  assert.equal(planRewind(points, 4), null, 'no point that far back')
  assert.equal(planRewind([], 1), null)
})

test('coverage gaps are said once each, in plain words, and an unrecognised one still warns', () => {
  assert.deepEqual(planRewind(points, 3).warnings, [
    'Commands that ran may have changed files that won’t be restored.',
    'Hooks that ran may have changed files that won’t be restored.',
  ])
  for (const gap of ['command', 'hook', 'scratch', 'language-server', 'desktop', 'backup-unavailable', 'unknown']) {
    assert.match(gapWarning(gap), /restored\.$/, gap)
    assert.doesNotMatch(gapWarning(gap), /\u2014/, 'no em-dash')
  }
  assert.equal(gapWarning('desktop') === gapWarning('unknown'), false)
  assert.equal(gapWarning('a-gap-from-a-newer-agent'), gapWarning('unknown'))
  assert.equal(gapWarning('toString'), gapWarning('unknown'), 'a prototype key is not a gap')
})

test('only a prompt whose turn a point undoes is offered the rewind, matched on its ordinal', () => {
  assert.equal(pointForPrompt(points, 3).steps, 1)
  assert.equal(pointForPrompt(points, 2).steps, 2)
  assert.equal(pointForPrompt(points, 0), null, 'older than every point')
  assert.equal(pointForPrompt(points, undefined), null, 'a prompt not yet numbered')
  // The third point's prompt has left the conversation, so no row can claim it.
  assert.equal(pointForPrompt(points, null), null)
})

test('the right-click menu holds a rewind only on a rewindable prompt, greyed while a turn runs', () => {
  const ids = (target) => CONTEXT[target].map((item) => item.id)
  assert.ok(ids('entry-user-rewindable').includes('context.entry.rewind'))
  assert.ok(!ids('entry-user').includes('context.entry.rewind'))
  assert.ok(!ids('entry').includes('context.entry.rewind'))
  const rewind = CONTEXT['entry-user-rewindable'].find((item) => item.id === 'context.entry.rewind')
  const idle = { ...NOTHING_OPEN, hasSession: true, canRewind: true }
  assert.equal(isEnabled(rewind.requires, idle), true)
  assert.equal(isEnabled(rewind.requires, { ...idle, running: true, canRewind: false }), false)
  assert.equal(command('turn.rewind').accelerator, undefined, 'Cmd+Z stays with text editing')
  assert.equal(isEnabled(command('turn.rewind').requires, NOTHING_OPEN), false)
  assert.equal(parseWindowState({ ...idle, canRewind: undefined }), null, 'a state that leaves it out enables nothing')
  assert.equal(parseWindowState(idle).canRewind, true)
})

test('Undo turn sits on the latest turn’s footer only while the newest point is that turn’s', () => {
  const live = [t.userSaid('first'), t.replied('one', 3), t.userSaid('second'), t.replied('two', 4)]
  assert.equal(undoRow(live, points), live[3].id)
  assert.equal(undoRow(live, points.slice(1)), null, 'the newest point is an older turn')
  assert.equal(undoRow(live, []), null)
  const failed = [...live, t.userSaid('third'), { ...t.errored('chat: broke'), turn: 5 }]
  assert.equal(undoRow(failed, [{ ...points[0], turn: 5, prompt: 4 }]), failed.at(-1).id)
  assert.equal(undoRow(failed, points), null, 'the failed turn left no point of its own')
  // A saved conversation draws replies with no turn numbers, so the prompt above decides.
  const saved = t.fromSaid([
    { kind: 'user', text: 'third', prompt: 2 }, { kind: 'assistant', text: 'three' },
    { kind: 'user', text: 'fourth', prompt: 3 }, { kind: 'assistant', text: 'four' },
  ])
  assert.equal(undoRow(saved, points), saved[3].id)
  assert.equal(undoRow(saved.slice(0, 2), points), null)
  assert.equal(undoRow(saved, [{ ...points[0], prompt: null }]), null)
})

test('a rewind refused because a turn is running says nothing was undone, and any other failure says what happened', () => {
  assert.equal(
    rewindFailure(new RequestFailed('turn_in_flight', 'a turn is running')),
    'A turn started, so nothing was undone. Try again once it finishes.',
  )
  assert.equal(
    rewindFailure(new RequestFailed('bad_request', 'This session can go back 2 turns at most.')),
    'Error: bad_request: This session can go back 2 turns at most.',
  )
  assert.equal(
    rewindFailure(new RequestFailed('bad_request', 'the words turn_in_flight appear here')),
    'Error: bad_request: the words turn_in_flight appear here',
    'the code decides, not a substring of the message',
  )
})

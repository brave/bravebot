// `/goal` typed into the composer (CMD-1, GOAL-1), and the step the window asks for after each turn.
//
// The bridge holds the goal and decides each step (crates/ui-bridge/src/goal.rs). What the window
// owns is which lines go there at all, that the answer is a row and never a prompt, and when it asks
// for the next step: after a turn under a goal, or a check that sent the work back, once nothing runs
// and no prompt the person queued is waiting. The faults these reject are a longer word taken for the
// command, a /goal line sent as a prompt, a step asked before the person's queued prompt, a step
// asked twice, and a check the stop key cannot reach.
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

const shared = load("export * from './src/shared/goal'")
const staging = load("export { exitOf } from './src/renderer/staging'")
const hook = load("export { goalChanged, checkBegun, checkEnded, judged, turnEnded, mayContinue } from './src/renderer/use-goal'")

const snapshot = (fields = {}) => ({
  condition: 'a.txt exists', rounds: 0, left: 10, paused: false, stalled: false, lastReason: null,
  elapsedSeconds: 0, spentTokens: 0, ...fields,
})
const live = (fields = {}) => ({
  handle: 's', entries: [], turns: {}, queued: [], interjected: [], running: false, queuePaused: false,
  summary: { id: null, title: 't', project: '/p', branch: null, directory: '/p' },
  archived: 0, awaitingOrdinal: null, staging: { made: 0, staged: [] }, agents: [], todos: [], quarantine: [], phase: null,
  ...fields,
})
const verdict = (fields = {}) => ({
  verdict: 'not-met', outcome: 'carry-on', next: 'carry-on', reason: 'a.txt is missing',
  line: 'The goal is not met yet: a.txt is missing', rounds: 1, left: 9, elapsedSeconds: 4, spentTokens: 120,
  goal: snapshot({ rounds: 1, left: 9, lastReason: 'a.txt is missing' }), ...fields,
})

test('only the command word, alone or with an argument, is /goal', () => {
  assert.equal(shared.goalArgument('/goal a.txt exists'), 'a.txt exists')
  assert.equal(shared.goalArgument('  /goal  x '), 'x')
  // Alone, it is the command with an empty argument, which the bridge answers with a report.
  assert.equal(shared.goalArgument('/goal'), '')
  assert.equal(shared.goalArgument('/goal clear'), 'clear')
  for (const prompt of ['/goals', '/goals x', 'set a /goal x', 'goal x', '/btw /goal x', '']) {
    assert.equal(shared.goalArgument(prompt), null, prompt)
  }
})

test('exitOf routes a typed /goal line to the goal, and a longer word to a prompt', () => {
  assert.deepEqual(staging.exitOf(undefined, '/goal  a.txt exists '), { to: 'goal', argument: 'a.txt exists', line: '/goal  a.txt exists', staged: [] })
  assert.deepEqual(staging.exitOf(undefined, '/goal'), { to: 'goal', argument: '', line: '/goal', staged: [] })
  assert.equal(staging.exitOf(undefined, '/goals').to, 'prompt')
  assert.equal(staging.exitOf(undefined, 'create a.txt').to, 'prompt')
  assert.equal(staging.exitOf(undefined, '/btw /goal x').to, 'aside')
})

test('the composer sends a /goal line to the goal in each place it sends one, before send, interject or the queue', () => {
  const turn = readFileSync('src/renderer/use-turn.ts', 'utf8')
  for (const name of ['const queueDraft = ', 'const submit = ', 'const interjectDraft = ']) {
    const start = turn.indexOf(name)
    const body = turn.slice(start, turn.indexOf('\n  }', start))
    const sent = body.search(/void goal\(\w+(?:\.current)?, exit\.argument, exit\.staged\)/)
    assert.ok(sent > 0, `${name} sends the goal`)
    assert.ok(body.indexOf("exit.to === 'goal'") < sent, `${name} reads the exit`)
    for (const later of ['send(', 'interject(', 'route(', 'queued:']) {
      const at = body.indexOf(later)
      assert.ok(at < 0 || at > sent, `${name} sends the goal before ${later}`)
    }
  }
  const goal = readFileSync('src/renderer/use-goal.ts', 'utf8')
  const command = goal.slice(goal.indexOf('const command = '), goal.indexOf('const receive = '))
  // The answer is drawn as a row, and the line is never a prompt.
  assert.match(command, /call<GoalAnswer>\('session\.goal', \{ session: handle, argument, \.\.\.markersOf\(staged\) \}\)/)
  assert.match(command, /t\.narrated\(answer\.lines\.join\('\\n'\), '\/goal'\)/)
  assert.doesNotMatch(command, /turn\.send|turn\.interject|\bsend\(/)
})

test('a turn that ends under a goal in force has a step to ask for, and one under a paused goal or none does not', () => {
  assert.equal(hook.turnEnded(live({ goal: snapshot() })).goalNext, true)
  assert.notEqual(hook.turnEnded(live({ goal: snapshot({ paused: true }) })).goalNext, true)
  assert.notEqual(hook.turnEnded(live({ goal: null })).goalNext, true)
  assert.notEqual(hook.turnEnded(live()).goalNext, true)
})

test('the step waits until nothing runs, no question waits, and no queued prompt is waiting or on its way', () => {
  const due = live({ goal: snapshot(), goalNext: true })
  assert.equal(hook.mayContinue(due, false), true)
  assert.equal(hook.mayContinue({ ...due, goalNext: false }, false), false, 'nothing is due')
  assert.equal(hook.mayContinue({ ...due, running: true }, false), false, 'a turn runs')
  assert.equal(hook.mayContinue({ ...due, compacting: true }, false), false, 'a compaction runs')
  assert.equal(hook.mayContinue({ ...due, askingTrust: { directory: '/p' } }, false), false, 'a question waits')
  assert.equal(hook.mayContinue({ ...due, queued: [{ prompt: 'mine', attachments: [], staged: [] }] }, false), false, 'a prompt waits')
  assert.equal(hook.mayContinue(due, true), false, 'a queued prompt is on its way to the bridge')
})

test('the window asks for the step once, from the effect, and clears it before the request', () => {
  const goal = readFileSync('src/renderer/use-goal.ts', 'utf8')
  assert.equal([...goal.matchAll(/'session\.goal\.continue'/g)].length, 1)
  const effect = goal.slice(goal.indexOf('useEffect(() => {'))
  assert.ok(effect.indexOf('mayContinue(item, dequeuing.current.has(item.handle))') > 0)
  const cleared = effect.indexOf('goalNext: false')
  assert.ok(cleared > 0 && cleared < effect.indexOf("'session.goal.continue'"), 'cleared before the request, so it is asked once')
  // The queue and the step share one record of the prompts on their way.
  const turn = readFileSync('src/renderer/use-turn.ts', 'utf8')
  assert.doesNotMatch(turn, /const dequeuing = useRef/)
  const app = readFileSync('src/renderer/App.tsx', 'utf8')
  assert.match(app, /const dequeuing = useRef\(new Set<string>\(\)\)/)
  assert.match(app, /useGoal\(\{ openedLives, updateSession, dequeuing, live, livesRevision \}\)/)
  assert.match(app, /dequeuing, goal: goals\.command,/)
})

test('a check is drawn as a turn, so the stop key reaches it, and ends with its verdict as a row', () => {
  const checking = hook.checkBegun(live({ goal: snapshot(), goalNext: true }))
  assert.equal(checking.running, true, 'Stop and Escape act on a running chat, and send turn.cancel')
  assert.equal(checking.goalChecking, true)
  assert.equal(checking.goalNext, false)
  assert.equal(checking.entries.at(-1).kind, 'narration')
  assert.equal(checking.entries.at(-1).command, '/goal')

  const after = hook.judged(checking, verdict())
  assert.equal(after.running, false)
  assert.equal(after.goalChecking, false)
  assert.equal(after.goalNext, true, 'a NOT MET asks for the turn that carries the work on')
  assert.equal(after.entries.at(-1).text, 'The goal is not met yet: a.txt is missing')
  assert.equal(after.entries.filter((entry) => entry.kind === 'user').length, 0, 'the verdict is no prompt')
  assert.deepEqual(after.goal, verdict().goal)

  for (const next of ['ended', 'stalled']) {
    const ended = hook.judged(checking, verdict({ next, outcome: next === 'ended' ? 'met' : 'stalled', goal: next === 'ended' ? null : snapshot({ stalled: true }) }))
    assert.equal(ended.goalNext, false, next)
    assert.equal(ended.running, false, next)
  }
  const failed = hook.judged(checking, verdict({ verdict: 'failed', outcome: 'failed', next: 'ended', goal: null, line: 'The goal could not be checked (x), so it is cleared.' }))
  assert.equal(failed.entries.at(-1).refused, true)
})

test('a check the stop key ended frees the chat once the goal is cleared, and a clear mid-turn leaves the turn alone', () => {
  const stopped = hook.goalChanged(hook.checkBegun(live({ goal: snapshot() })), null)
  assert.equal(stopped.running, false)
  assert.equal(stopped.goalChecking, false)
  assert.equal(stopped.goal, null)
  const midTurn = hook.goalChanged(live({ running: true, goal: snapshot() }), null)
  assert.equal(midTurn.running, true, 'a /goal clear typed during a turn does not end the turn')
  assert.equal(hook.goalChanged(live({ goal: snapshot(), goalNext: true }), null).goalNext, false, 'no step is asked for a goal that is gone')
})

test('the stop key sends turn.cancel, which the bridge applies to a check that is out', () => {
  const turn = readFileSync('src/renderer/use-turn.ts', 'utf8')
  const cancel = turn.slice(turn.indexOf('const cancel = '), turn.indexOf('const answer = useCallback'))
  assert.match(cancel, /call\('turn\.cancel', \{ session: handle \}\)/, 'with no target, which the bridge reads as the check')
})

test('the goal events and turn.done reach the hook', () => {
  const app = readFileSync('src/renderer/App.tsx', 'utf8')
  assert.match(app, /message\.event === 'goal\.changed' \|\| message\.event === 'goal\.checking' \|\| message\.event === 'goal\.judged'\) && message\.session\) goals\.receive\(message\.session, message\)/)
  assert.match(app, /if \(message\.event === 'turn\.done' && message\.session\) goals\.receive\(message\.session, message\)/)
})

test('the bridge methods are ones the window may send', () => {
  const index = readFileSync('src/main/index.ts', 'utf8')
  assert.match(index, /'session\.goal',\s*'session\.goal\.continue',/)
})

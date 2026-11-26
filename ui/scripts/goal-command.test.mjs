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
  assert.match(command, /t\.goalNoticed\(answer\.lines\.join\('\\n'\), answerTone\(answer\)\)/)
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
  assert.equal(checking.entries.length, 0, 'the working row says it is a check, so no row is added')

  const after = hook.judged(checking, verdict())
  assert.equal(after.running, false)
  assert.equal(after.goalChecking, false)
  assert.equal(after.goalNext, true, 'a NOT MET asks for the turn that carries the work on')
  assert.equal(after.entries.at(-1).text, 'The goal is not met yet: a.txt is missing')
  assert.equal(after.entries.at(-1).kind, 'goal-notice')
  assert.equal(after.entries.length, 1, 'one row per check')
  assert.equal(after.entries.filter((entry) => entry.kind === 'user').length, 0, 'the verdict is no prompt')
  assert.deepEqual(after.goal, verdict().goal)

  for (const next of ['ended', 'stalled']) {
    const ended = hook.judged(checking, verdict({ next, outcome: next === 'ended' ? 'met' : 'stalled', goal: next === 'ended' ? null : snapshot({ stalled: true }) }))
    assert.equal(ended.goalNext, false, next)
    assert.equal(ended.running, false, next)
  }
  const failed = hook.judged(checking, verdict({ verdict: 'failed', outcome: 'failed', next: 'ended', goal: null, line: 'The goal could not be checked (x), so it is cleared.' }))
  assert.equal(failed.entries.at(-1).tone, 'refused')
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
  assert.match(app, /message\.event === 'goal\.changed' \|\| message\.event === 'goal\.checking' \|\| message\.event === 'goal\.judged' \|\| message\.event === 'goal\.carried'\) && message\.session\) goals\.receive\(message\.session, message\)/)
  assert.match(app, /if \(message\.event === 'turn\.done' && message\.session\) goals\.receive\(message\.session, message\)/)
})

test('the bridge methods are ones the window may send', () => {
  const index = readFileSync('src/main/index.ts', 'utf8')
  assert.match(index, /'session\.goal',\s*'session\.goal\.continue',/)
})

// The carry-on prompt and the goal panel (GOAL-7, GOAL-13), drawn through `react-dom/server` as
// aside-command.test.mjs draws the Asides panel. The faults these reject are the driver's sentence
// drawn as the person's bubble, a turn's notices placed above the sentence that started it, and a
// paused or stalled goal the panel does not say is paused or stalled.
const transcript = load("export * from './src/renderer/transcript'")
const React = require('react')
const { renderToStaticMarkup } = require('react-dom/server')
const withoutTree = {
  name: 'without-tree',
  setup(built) {
    built.onResolve({ filter: /^\.\/FileTree$/ }, () => ({ path: 'tree', namespace: 'no-tree' }))
    built.onLoad({ filter: /.*/, namespace: 'no-tree' }, () => ({ loader: 'js', contents: 'export const FileTree = () => null' }))
  },
}
let components
const loaded = async () => {
  const { loadComponents } = await import('./render-support.mjs')
  return (components ??= loadComponents({
    contents: "export { Context } from './src/renderer/components/Context.tsx'\nexport { Row, GOAL_ICONS } from './src/renderer/components/Transcript.tsx'\nexport * as transcript from './src/renderer/transcript'",
  }, [withoutTree]))
}

test('the turn goal.carried starts is placed under the driver\'s row, and no summoned bot answers it', () => {
  const carried = transcript.goalCarried('Carry on.\n\nCondition: a.txt exists')
  assert.equal(carried.kind, 'goal-turn')
  assert.equal(carried.text, 'Carry on.\n\nCondition: a.txt exists')
  const entries = transcript.beginTurn([transcript.userSaid('hi'), transcript.narrated('Checked'), carried], 2)
  assert.equal(entries.at(-1).kind, 'turn-start', 'the turn starts below the driver\'s sentence')
  assert.equal(transcript.summonedFor([transcript.userSaid('hi', 'bot-a'), carried]), undefined)
  assert.equal(transcript.plainText(carried), carried.text, 'it copies as its text')
})

test('the hook adds the driver\'s row on goal.carried', () => {
  const source = readFileSync('src/renderer/use-goal.ts', 'utf8')
  assert.match(source, /case 'goal\.carried': return \{ \.\.\.old, entries: \[\.\.\.old\.entries, t\.goalCarried\(message\.data\.prompt\)\] \}/)
})

test('the carry-on prompt is drawn as the driver\'s row, not as a line the person typed', async () => {
  const { Row, transcript: own } = await loaded()
  const markup = renderToStaticMarkup(React.createElement(Row, {
    entry: own.goalCarried('Carry on <b>now</b>'), onDecide() {}, onAnswer() {}, onFork() {}, forkable: false,
  }))
  assert.match(markup, /data-test="goal-turn"/)
  assert.doesNotMatch(markup, /bubble user/, 'not the person\'s bubble')
  assert.ok(markup.includes('Automatic turn · sent by /goal'), markup)
  assert.ok(markup.includes('Carry on &lt;b&gt;now&lt;/b&gt;'), 'the sentence is text')
  // The head is the row; the sentence waits under a collapsed Details, as on a decision card.
  // Leo's elements take their attributes once mounted, so the fold is read from the source.
  assert.match(markup, /<\/span><div class="goal-turn-details"><leo-collapse><p class="goal-turn-prompt">/, markup)
  const source = readFileSync('src/renderer/components/Transcript.tsx', 'utf8')
  assert.match(source, /<Collapse className="card-details" title="Details" isOpen=\{undefined\}>\s*<p className="goal-turn-prompt">\{entry\.text\}<\/p>/)
})

const goalPanel = async (goal) => {
  const { Context } = await loaded()
  const inspected = { handle: 's-1', summary: { directory: '/p' }, entries: [], todos: [], quarantine: [], phase: null, tokens: 0, running: false, goal }
  const markup = renderToStaticMarkup(React.createElement(Context, { live: inspected }))
  const start = markup.indexOf('id="panel-goal"')
  return start < 0 ? null : markup.slice(start, markup.indexOf('</section>', start))
}

test('the goal panel shows the condition, rounds left, time set and tokens spent', async () => {
  const panel = await goalPanel(snapshot({ rounds: 3, left: 7, elapsedSeconds: 125, spentTokens: 4200, lastReason: 'a.txt is missing' }))
  assert.ok(panel, 'the column has a Goal panel while a goal is set')
  assert.ok(panel.includes('a.txt exists'), panel)
  assert.ok(panel.includes('7 of 10'), panel)
  assert.ok(panel.includes('2m 05s'), panel)
  assert.ok(panel.includes('4.2k tokens'), panel)
  assert.ok(panel.includes('a.txt is missing'), panel)
  assert.ok(panel.includes('Working until a check says it holds.'), panel)
  assert.ok(panel.includes('Running for') && panel.includes('>Spent<'), panel)
  assert.equal(await goalPanel(null), null, 'no panel without a goal')
})

test('a paused or stalled goal says so in words', async () => {
  const paused = await goalPanel(snapshot({ paused: true }))
  assert.match(paused, /goal-state paused/)
  assert.ok(paused.includes('Paused. Type <code>/goal resume</code> to carry on.'), paused)
  const stalled = await goalPanel(snapshot({ stalled: true }))
  assert.match(stalled, /goal-state stalled/)
  assert.ok(stalled.includes('Stopped: rounds in a row made no tool call'), stalled)
})

// The goal's notices. The faults these reject are a notice whose mark does not say what the
// check came to, a mark read from the sentence's words, and a command half drawn as code.
test('each goal notice is marked from the fields the bridge sent, not from its sentence', () => {
  assert.equal(shared.verdictTone('met'), 'met')
  for (const outcome of ['impossible', 'spent', 'stalled']) assert.equal(shared.verdictTone(outcome), 'warning', outcome)
  assert.equal(shared.verdictTone('failed'), 'refused')
  for (const outcome of ['carry-on', 'unreadable', 'quarantined']) assert.equal(shared.verdictTone(outcome), 'plain', outcome)
  assert.equal(shared.answerTone({ goal: snapshot({ paused: true }), changed: true, lines: ['The goal is met.'] }), 'paused')
  assert.equal(shared.answerTone({ goal: snapshot(), changed: true, lines: ['The goal is paused.'] }), 'plain')
  assert.equal(shared.answerTone({ goal: null, changed: true, lines: [] }), 'plain')
})

test('a goal notice draws its mark, and every whole command it names as one code span', async () => {
  const { Row, GOAL_ICONS, transcript: own } = await loaded()
  const draw = (entry) => renderToStaticMarkup(React.createElement(Row, { entry, onDecide() {}, onAnswer() {}, onFork() {}, forkable: false }))
  const paused = draw(own.goalNoticed('Turns are not judged until /goal resume, and /goal clear still takes it off. /goal resume.', 'paused'))
  assert.match(paused, /class="goal-notice paused"/)
  assert.equal([...paused.matchAll(/<code>\/goal resume<\/code>/g)].length, 2, paused)
  assert.equal([...paused.matchAll(/<code>\/goal clear<\/code>/g)].length, 1, paused)
  assert.doesNotMatch(paused, /<code>\/goal<\/code>/, 'never part of a command')
  assert.match(draw(own.goalNoticed('The goal is met: a.txt is there', 'met')), /class="goal-notice met"/)
  // `plain`, not `goal`: a notice's class must not be the Goal panel's `.goal`.
  assert.match(draw(own.goalNoticed('Working towards: x', 'plain')), /class="goal-notice plain"/)
  assert.deepEqual(GOAL_ICONS, { plain: 'flag', met: 'check-circle-filled', warning: 'warning-triangle-outline', paused: 'pause-circle', refused: 'warning-triangle-outline' })
  assert.ok(draw(own.goalNoticed('<b>x</b> /goal clear', 'plain')).includes('&lt;b&gt;x&lt;/b&gt; <code>/goal clear</code>'), 'the sentence is text')
})

test('a check is drawn in the working row with its own word and no token count', () => {
  const source = readFileSync('src/renderer/components/Transcript.tsx', 'utf8')
  assert.match(source, /word=\{live\.goalChecking \? \(\) => GOAL_CHECK_WORD : /)
  assert.match(source, /tokens=\{live\.goalChecking \? 0 : live\.tokens\}/)
  assert.match(source, /export const GOAL_CHECK_WORD = 'Checking whether the goal is met'/)
})

/**
 * What the agent can do to a conversation, one scenario each. The panel lists them and a typed
 * prompt can pick one by a word in it. A scenario only announces things and waits for answers; the
 * transcript draws them exactly as it draws a real agent's.
 */
import type { EventMap, TodoRow } from '../src/shared/protocol'
import {
  ALL_ENTRIES_REPLY, ASK, EXPOSURE, FAILURES, FETCH, FETCH_TRICKY, HOST, MANIFEST, MCP_CALL, MCP_MOVE, MCP_SERVER, MCP_SERVER_REMOTE, MCP_TOOLS,
  NO_MODEL, OUTPUT, OUTPUT_UNSAFE, PLAN_DECLINED, PLAN_FAILED, PLAN_STOPPED, QUARANTINE, RUN_ONE, RUN_PIPE, SERVER, SERVER_DECLARED, TODOS, VET,
  VET_PICTURE, VOUCH, WRITE_CREATE, WRITE_EDIT, WRITE_OVERWRITE, WRITE_UNTRUSTED, tool,
} from './fixtures'
import type { Described, Kind, Scenario, Script, Want } from './script'

const REPLY = `Split the lock into a read side and a write side, moved the migration into \`migrate.ts\`, and added **three** tests.

\`\`\`ts
export function open(path: string): Cache {
  const cache = new Cache(path)
  cache.migrate()
  return cache
}
\`\`\`

The full suite passes.`

type RequestEvent = {
  [N in keyof EventMap]: EventMap[N] extends { request: number } ? N : never
}[keyof EventMap]

interface Card<N extends RequestEvent = RequestEvent> {
  event: N
  kind: Kind
  data: (request: number) => EventMap[N]
}

const card = <N extends RequestEvent>(event: N, kind: Kind, data: (request: number) => EventMap[N]): Card<N> => ({ event, kind, data })
const withId = <T extends { request: number }>(base: T) => (request: number): T => ({ ...base, request })

const CONFIRM = (base: EventMap['confirm.request']): Card => card('confirm.request', 'confirm', withId(base))

/** Draw a card, wait for the answer, and say what the agent did with it. */
async function ask(s: Script, c: Card, want: Want = 'approve'): Promise<'approve' | 'reject'> {
  const request = s.id()
  s.emit(c.event, c.data(request) as never)
  const answered = await s.ask(c.kind, request, want)
  return answered.decision
}

const planning = async (s: Script, ms = 700): Promise<void> => {
  s.emit('phase', { phase: 'planning' })
  await s.wait(ms)
}

/** One call, from start to its result. */
async function call(s: Script, verb: string, target: string, why: string, note: string, extra: Partial<ReturnType<typeof tool>> = {}, landing: EventMap['landed']['landing'] = 'context'): Promise<void> {
  s.emit('tool.started', tool(verb, target, why, null))
  await s.wait(700)
  s.emit('tool.finished', tool(verb, target, why, note, extra))
  s.emit('landed', { landing })
}

// ---------------------------------------------------------------- turns

/** The reply a typed prompt gets by default: a look at a file and an answer. */
export const QUICK: Scenario = async (s) => {
  s.start()
  await planning(s, 500)
  await call(s, 'Read', 'src/cache.ts', 'See how the cache is laid out', '120 lines, read', { waitedSeconds: 2 })
  s.emit('tokens', { written: 640 })
  await s.wait(500)
  s.done(`Here is what I found.\n\n${REPLY}`)
}

/** A whole turn: everything an agent does between a prompt and its reply. */
export const TOUR: Scenario = async (s) => {
  s.start()
  s.emit('phase', { phase: 'planning' })
  await s.wait(800)
  s.emit('phase', { phase: 'thinking' })
  const thought = ['The lock is taken in open(), so every reader waits on the writer.', 'A read side that takes a snapshot would not need it.', 'Check whether migrate() writes while a reader is open.']
  for (const line of thought) { s.emit('thinking', { text: `${line}\n` }); await s.wait(500) }
  s.emit('narration', { text: 'I will look at the cache first, then the vendor notes the README points to.' })
  await call(s, 'Read', 'src/cache.ts', 'Check the lock', '120 lines, read', { waitedSeconds: 4 })
  const status = (...each: TodoRow['status'][]): TodoRow[] => TODOS.slice(0, 3).map((row, index) => ({ ...row, status: each[index] ?? row.status }))
  s.emit('todos', { rows: status('active', 'pending', 'pending') })
  await call(s, 'Read', 'vendor/notes.md', 'Look at the vendored notes', '48 lines, confined', {}, 'quarantined')
  s.emit('quarantined', QUARANTINE)
  await call(s, 'Read', 'fixtures/tides-2019.csv', 'Check the sample data', 'name only', {}, 'reserved')
  s.emit('quarantined', NO_MODEL)
  await call(s, 'Search', 'lock', 'Find other lock users', '3 matches')
  s.emit('tokens', { written: 1834 })
  s.emit('phase', { phase: 'thinking' })
  s.emit('composing', { call: 'Edit' })
  await s.wait(900)
  const write = await ask(s, CONFIRM(WRITE_EDIT), 'approve')
  if (write === 'approve') {
    await call(s, 'Edit', 'src/cache.ts', 'Split the lock', '+3 −1')
    s.emit('todos', { rows: status('done', 'active', 'pending') })
  } else s.emit('narration', { text: 'Understood. I left src/cache.ts as it was and will describe the change instead.' })
  const run = await ask(s, card('run.request', 'run', withId(RUN_ONE)), 'approve')
  if (run === 'approve') {
    s.emit('check.started', { lines: 6 })
    await s.wait(900)
    s.emit('check.finished', {})
    const output = await ask(s, card('output.request', 'output', withId(OUTPUT)), 'approve')
    s.emit('narration', { text: output === 'approve' ? 'The tests pass. I read their output.' : 'I will not read the output; I will go by the exit code.' })
  }
  await call(s, 'Run', 'npm test -- cache', 'Run the cache tests', run === 'approve' ? 'exit 0' : 'not run', run === 'approve' ? {} : { failed: true })
  s.emit('tokens', { written: 4210 })
  s.done(REPLY, { notices: ['Hook turn-finished: formatted 2 files'], steps: 7, rewind: true })
}

// ---------------------------------------------------------------- what the working row says

const working = (run: (s: Script) => Promise<void>): Scenario => async (s) => {
  s.start()
  await run(s)
  await s.hold()
}

const WORKING: Described[] = [
  { id: 'phase-planning', title: 'Planning', run: working(async (s) => s.emit('phase', { phase: 'planning' })) },
  { id: 'phase-thinking', title: 'Thinking, with the thinking tail', run: working(async (s) => {
    s.emit('phase', { phase: 'thinking' })
    const lines = ['The lock is taken in open(), so every reader waits on the writer.', 'A read side that takes a snapshot would not need it.', 'Check whether migrate() writes while a reader is open.', ...Array.from({ length: 8 }, (_, step) => `Step ${step + 1}: read the next call site and note what it holds.`)]
    for (const line of lines) { s.emit('thinking', { text: `${line}\n` }); await s.wait(150) }
  }) },
  { id: 'phase-compacting', title: 'Compacting the conversation', run: working(async (s) => s.emit('phase', { phase: 'compacting' })) },
  { id: 'phase-reconnecting', title: 'Reconnecting', run: working(async (s) => s.emit('phase', { phase: 'reconnecting' })) },
  { id: 'phase-servers', title: 'Starting MCP servers', run: working(async (s) => { s.emit('mcp.starting', { servers: ['weather', 'tides'] }) }) },
  { id: 'check-lines', title: 'A confined check on text', run: working(async (s) => { s.emit('phase', { phase: 'thinking' }); s.emit('check.started', { lines: 482 }) }) },
  { id: 'check-picture', title: 'A confined check on a picture', run: working(async (s) => { s.emit('phase', { phase: 'thinking' }); s.emit('check.started', { file: 'picture' }) }) },
  { id: 'hook', title: 'A hook holding the turn open', run: working(async (s) => { s.emit('hook.started', { moment: 'tool-finished', program: './scripts/format.sh src/cache.ts' }) }) },
  { id: 'composing', title: 'Writing a tool call', run: working(async (s) => { s.emit('phase', { phase: 'thinking' }); s.emit('composing', { call: 'Edit' }) }) },
  { id: 'agents', title: 'Delegates at work', run: working(async (s) => {
    s.emit('phase', { phase: 'planning' })
    s.emit('agents', { started: 3, working: 2, agents: [
      { id: '1', kind: 'explore', task: 'Find every caller of Cache.open', state: 'finished' },
      { id: '2', kind: 'review', task: 'Check the migration against the old schema', state: 'working' },
      { id: '3', kind: 'tests', task: 'Write a regression test for the lock', state: 'working' },
    ] })
  }) },
  { id: 'busy', title: 'Todos, tokens and a running call', run: working(async (s) => {
    s.emit('phase', { phase: 'planning' })
    s.emit('todos', { rows: TODOS })
    s.emit('tokens', { written: 3_204 })
    s.emit('tool.started', tool('Search', 'RwLock', 'Find other lock users', null))
  }) },
].map((entry) => ({ ...entry, group: 'Working row', detail: 'Stays until you stop it.' }))

// ---------------------------------------------------------------- the calls an agent makes

const TOOLS: Scenario = async (s) => {
  s.start()
  await planning(s, 400)
  await call(s, 'Read', 'src/cache.ts', 'Check the lock', '120 lines, read', { waitedSeconds: 4 })
  await call(s, 'Read', 'docs/a/very/deeply/nested/folder/that/goes/on/and/on/and/on/until/the/path/has/to/be/truncated.md', 'A path long enough that the middle must give way', '9 lines, read')
  await call(s, 'Read', 'vendor/notes.md', 'Look at the vendored notes', '48 lines, confined', {}, 'quarantined')
  s.emit('quarantined', QUARANTINE)
  await call(s, 'Read', 'fixtures/tides-2019.csv', 'Too large and not for any model', 'name only', {}, 'reserved')
  s.emit('quarantined', NO_MODEL)
  await call(s, 'Search', 'lock', 'Find other lock users', '3 matches')
  await call(s, 'Fetch', 'https://tides.example.org/api', 'Get the live table', 'timed out after 30s', { failed: true })
  await call(s, 'Edit', 'src/cache.ts', 'Split the lock', 'refused: you said no', { failed: true })
  s.emit('tool.started', tool('Run', 'npm test', 'Still going when the turn ends', null))
  await s.wait(600)
  s.done('Those are the kinds of call a turn shows: finished, failed, confined, name only and still running.')
}

const NARRATION: Scenario = async (s) => {
  s.start()
  await planning(s, 400)
  s.emit('narration', { text: 'Starting with the failing test.' })
  s.emit('narration', { text: 'The check could not run. Install the missing tool first:', command: 'brew install tide-tools' })
  s.emit('watch.fired', { number: 1, path: 'src/cache.ts' })
  s.emit('watch.ended', { number: 1, reason: 'expired', message: 'It ran for 24 hours' })
  s.emit('mcp.started', { servers: ['weather'], confined: true, notes: ['tides did not answer its handshake within a minute, so it was left out.'] })
  s.done('Those are the lines that sit between the conversation’s own words.')
}

const COMPACTION: Scenario = async (s) => {
  s.start()
  s.emit('phase', { phase: 'compacting' })
  await s.wait(1600)
  s.done('I summarised the earlier part of this conversation to make room. What you asked for is unchanged.', { notices: ['Compacted 38 messages into a summary'] })
}

const WATCH: Scenario = async (s) => {
  s.emit('watch.fired', { number: 2, path: 'src/cache.ts' })
  s.start()
  await planning(s, 500)
  await call(s, 'Read', 'src/cache.ts', 'See what changed', '121 lines, read')
  s.done('`src/cache.ts` gained one line: the new `readLock` field.')
}

const BUDGET: Scenario = async (s) => {
  s.start()
  await planning(s, 500)
  s.emit('tokens', { written: 9_000 })
  s.done('Finished. Nothing to approve.', { notices: ['Hook turn-finished: formatted 2 files', 'Backups kept for 3 files'] })
}

// ---------------------------------------------------------------- every card

interface CardCase { id: string; title: string; detail: string; card: Card; want?: Want }

const CARDS: CardCase[] = [
  { id: 'write-edit', title: 'Write: edit a file', detail: 'A diff with kept, removed, added and skipped lines.', card: CONFIRM(WRITE_EDIT) },
  { id: 'write-create', title: 'Write: create a file', detail: 'Every line is new.', card: CONFIRM(WRITE_CREATE) },
  { id: 'write-overwrite', title: 'Write: overwrite, inexact', detail: 'Approximate diff, line endings, written since checkout.', card: CONFIRM(WRITE_OVERWRITE) },
  { id: 'write-untrusted', title: 'Write: untrusted source', detail: 'Credential finding, processor remark, hidden characters.', card: CONFIRM(WRITE_UNTRUSTED) },
  { id: 'run-one', title: 'Run: one command', detail: 'Confined, can be trusted.', card: card('run.request', 'run', withId(RUN_ONE)), want: 'always' },
  { id: 'run-pipe', title: 'Run: pipeline', detail: 'Three stages, a write, lent scopes, ambient access, releases private data, not confined.', card: card('run.request', 'run', withId(RUN_PIPE)) },
  { id: 'output', title: 'Output: safe to read', detail: 'Whether the model may read what a command printed.', card: card('output.request', 'output', withId(OUTPUT)) },
  { id: 'output-unsafe', title: 'Output: flagged', detail: 'The check found an instruction in it.', card: card('output.request', 'output', withId(OUTPUT_UNSAFE)), want: 'reject' },
  { id: 'vouch', title: 'Vouch for a file', detail: 'A confined file the model would like to read.', card: card('vouch.request', 'vouch', withId(VOUCH)), want: 'reject' },
  { id: 'vet-text', title: 'Vet: text', detail: 'Content a check let through, put to you before the model reads it.', card: card('vet.request', 'vet', withId(VET)) },
  { id: 'vet-picture', title: 'Vet: picture', detail: 'A picture held for you to look at.', card: card('vet.request', 'vet', withId(VET_PICTURE)) },
  { id: 'fetch', title: 'Fetch a URL', detail: 'The host you would be agreeing to.', card: card('fetch.request', 'fetch', withId(FETCH)) },
  { id: 'fetch-tricky', title: 'Fetch: a misleading URL', detail: 'Redirected host and the metadata service.', card: card('fetch.request', 'fetch', withId(FETCH_TRICKY)), want: 'reject' },
  { id: 'server', title: 'Start a language server', detail: 'It runs build tooling.', card: card('server.request', 'server', withId(SERVER)) },
  { id: 'server-declared', title: 'Start a language server you declared', detail: 'What it runs is not known.', card: card('server.request', 'server', withId(SERVER_DECLARED)) },
  { id: 'manifest', title: 'Run a frozen plan', detail: 'Every step, and the rules the plan is not held to.', card: card('manifest.request', 'manifest', withId(MANIFEST)) },
  { id: 'exposure', title: 'Read a file holding credentials', detail: 'Findings without any value.', card: card('exposure.request', 'exposure', withId(EXPOSURE)), want: 'reject' },
  { id: 'mcp-server', title: 'MCP server: local', detail: 'Command, variables, what it fetches.', card: card('mcp-server.request', 'mcp-server', withId(MCP_SERVER)), want: 'always' },
  { id: 'mcp-server-remote', title: 'MCP server: remote', detail: 'A URL and nothing stored.', card: card('mcp-server.request', 'mcp-server', withId(MCP_SERVER_REMOTE)) },
  { id: 'mcp-tools', title: 'MCP tools', detail: 'A server’s tool list, one with an instruction in it.', card: card('mcp-tools.request', 'mcp-tools', withId(MCP_TOOLS)) },
  { id: 'mcp-call', title: 'MCP call', detail: 'One call with its arguments.', card: card('mcp-call.request', 'mcp-call', withId(MCP_CALL)), want: 'always' },
  { id: 'mcp-move', title: 'MCP server moved', detail: 'A reply pointed somewhere else.', card: card('mcp-move.request', 'mcp-move', withId(MCP_MOVE)), want: 'reject' },
  { id: 'host', title: 'Network hosts', detail: 'Hosts a confined program asked for.', card: card('host.request', 'host', withId(HOST)) },
  { id: 'ask', title: 'Questions from the agent', detail: 'One at a time: pick one, pick several, answer in words.', card: card('ask.request', 'ask', withId(ASK)) },
]

const oneCard = (c: CardCase): Scenario => async (s) => {
  s.start()
  await planning(s, 400)
  const answer = await ask(s, c.card, c.want)
  s.emit('narration', { text: answer === 'approve' ? 'Thank you. Going ahead.' : 'Understood. I will not do that.' })
  s.done(answer === 'approve' ? 'That went ahead as you allowed.' : 'I did not do that, as you asked.')
}

const CARD_SCENARIOS: Described[] = CARDS.map((c) => ({ id: c.id, group: 'One question', title: c.title, detail: c.detail, run: oneCard(c) }))

/** Every card at once, none answered, which is also how each one looks when the turn is stopped. */
const WAITING: Scenario = async (s) => {
  s.start()
  await planning(s, 300)
  for (const c of CARDS) {
    s.emit(c.card.event, c.card.data(s.id()) as never)
    await s.wait(60)
  }
  await s.hold()
}

/** Every card, answered by the harness alternately yes and no, so each decided state is on screen. */
const ANSWERED: Scenario = async (s) => {
  s.start()
  await planning(s, 300)
  let flip = false
  for (const c of CARDS) {
    const want: Want = flip ? 'reject' : c.want === 'reject' ? 'approve' : c.want ?? 'approve'
    flip = !flip
    await ask(s, c.card, want)
  }
  s.done(ALL_ENTRIES_REPLY)
}

/** Questions the turn ended before anyone answered. */
const CUT_OFF: Scenario = async (s) => {
  s.start()
  await planning(s, 300)
  for (const c of [CARDS[0]!, CARDS[4]!, CARDS[8]!, CARDS[3]!]) s.emit(c.card.event, c.card.data(s.id()) as never)
  await s.wait(900)
  s.fail('chat', 'The connection closed before anyone answered.', { category: 'transport' })
}

// ---------------------------------------------------------------- when things go wrong

const failing = (label: string, category: string | null, message: string, extra: Record<string, unknown> = {}): Described => ({
  id: `error-${label.toLowerCase().replace(/[^a-z]+/g, '-')}`, group: 'Failures', title: label, detail: message,
  run: async (s) => {
    s.start()
    await planning(s, 400)
    await call(s, 'Read', 'src/cache.ts', 'Check the lock', '120 lines, read')
    s.fail('chat', message, { category, ...extra })
  },
})

const STOPPED: Scenario = async (s) => {
  s.start()
  await planning(s, 400)
  s.emit('tool.started', tool('Run', 'npm test', 'Run the suite', null))
  await s.hold()
}

// ---------------------------------------------------------------- plan runs

export const PLAN_RUN: Scenario = async (s) => {
  s.emit('manifest.started', { run: 1 })
  s.emit('phase', { phase: 'planning' })
  await s.wait(900)
  const answer = await ask(s, card('manifest.request', 'manifest', withId(MANIFEST)), 'approve')
  if (answer === 'reject') { s.emit('manifest.error', PLAN_DECLINED); return }
  for (const step of MANIFEST.steps.slice(0, 4)) {
    s.emit('tool.started', tool('Step', step, '', null))
    await s.wait(600)
    s.emit('tool.finished', tool('Step', step, '', 'done'))
  }
  s.emit('manifest.done', {
    run: 1, reply: 'Renamed `cache` to `store` in 6 files. The type checker reports no errors.\n\nNothing in this text was read by the planner.', model: 'sample/fast', steps: 5, clean: true, tokens: 9_000, outputTokens: 800,
    attempt: { goal: 'Rename the cache module', proposed: null, plan: 'search → edit → edit → check', steps: MANIFEST.steps }, record: '/Users/example/.bravebot/runs/1.json', trust: { rules: [] },
  })
}

const planEnds = (label: string, ended: typeof PLAN_DECLINED, detail: string): Described => ({
  id: `plan-${label.toLowerCase()}`, group: 'Plan runs', title: `A plan run: ${label.toLowerCase()}`, detail,
  run: async (s) => {
    s.emit('manifest.started', { run: ended.run })
    s.emit('phase', { phase: 'planning' })
    await s.wait(800)
    s.emit('manifest.error', ended)
  },
})

// ---------------------------------------------------------------- the list

export const SCENARIOS: Described[] = [
  { id: 'tour', group: 'Turns', title: 'A whole turn', detail: 'Thinking, calls, confined reads, a write, a command, its output and the reply. Answer the cards yourself.', run: TOUR },
  { id: 'quick', group: 'Turns', title: 'A short reply', detail: 'What a typed prompt gets unless it names another scenario.', run: QUICK },
  { id: 'tools', group: 'Turns', title: 'Every kind of call', detail: 'Finished, failed, confined, name only, long path, still running.', run: TOOLS },
  { id: 'narration', group: 'Turns', title: 'Notices between messages', detail: 'Narration with a command, a file watch, a watch ending, servers started.', run: NARRATION },
  { id: 'watch', group: 'Turns', title: 'A turn a file watch started', detail: 'The automatic-turn row.', run: WATCH },
  { id: 'compaction', group: 'Turns', title: 'Compaction', detail: 'The conversation is summarised.', run: COMPACTION },
  { id: 'notices', group: 'Turns', title: 'A reply with notices', detail: 'Hook output under the reply.', run: BUDGET },
  ...WORKING,
  ...CARD_SCENARIOS,
  { id: 'all-waiting', group: 'All at once', title: 'Every question, waiting', detail: 'All 24 cards unanswered. Stop the turn to see each one cut off.', run: WAITING },
  { id: 'all-answered', group: 'All at once', title: 'Every question, answered', detail: 'The harness presses yes and no in turn. Takes about a minute.', run: ANSWERED },
  { id: 'cut-off', group: 'All at once', title: 'Questions cut off', detail: 'Four cards, then the turn fails before any is answered.', run: CUT_OFF },
  ...FAILURES.map((failure) => failing(failure.label, failure.category, failure.message, failure.extra)),
  { id: 'error-stopped', group: 'Failures', title: 'Stopped by you', detail: 'Starts a call and holds. Press Stop in the composer.', run: STOPPED },
  planEnds('Declined', PLAN_DECLINED, 'The plan was put to you and refused.'),
  planEnds('Stopped', PLAN_STOPPED, 'You stopped the run.'),
  planEnds('Failed', PLAN_FAILED, 'A step failed.'),
]
